//! Autolinks
//!
//! `<https://example.org>`
//!
//! <https://spec.commonmark.org/0.30/#autolinks>
use std::sync::LazyLock;

use regex::Regex;

use crate::NodeDraft;
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};
use crate::parser::inline::{InlineRule, InlineState, LegacyInlineRule, TextSpecial};
use crate::parser::linkfmt::LinkFormatter;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

#[derive(Debug)]
pub struct Autolink {
    pub url: String,
}

struct AutolinkDocumentRenderer;

impl DocumentNodeRenderer<Autolink> for AutolinkDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Autolink,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        write_html_open(output, "a", &attrs)?;
        context.render_children(node.id(), output)?;
        write_html_close(output, "a")
    }
}

impl NodeValue for Autolink {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("href".into(), self.url.clone()));

        fmt.open("a", &attrs);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_migrated_rule::<AutolinkScanner>();
    md.add_document_renderer::<Autolink, _>("html", AutolinkDocumentRenderer);
    md.add_document_renderer::<Autolink, _>("text", TransparentDocumentRenderer);
}

static AUTOLINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.\-]{1,31}):([^<>\x00-\x20]*)$").unwrap());

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)$").unwrap()
});

#[doc(hidden)]
pub struct AutolinkScanner;
impl LegacyInlineRule for AutolinkScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["autolink"];

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let matched = scan_autolink(
            &state.src[state.pos..state.pos_max],
            state.md.link_formatter.as_ref(),
        )?;

        let start = state.pos;

        let mut child = Node::new(TextSpecial {
            content: matched.label.clone(),
            markup: matched.label,
            info: "autolink",
        });
        child.srcmap = state.get_map(start + matched.label_start, start + matched.label_end);

        let mut container = Node::new(Autolink {
            url: matched.destination,
        });
        container.children.push(child);

        Some((container, matched.consumed))
    }
}

struct AutolinkMatch {
    consumed: usize,
    label_start: usize,
    label_end: usize,
    destination: String,
    label: String,
}

impl InlineRule for AutolinkScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["autolink"];

    fn run(
        state: &mut crate::DocumentInlineState<'_>,
    ) -> Option<(Option<crate::NodeDraft>, usize)> {
        let matched = scan_autolink(
            state.remaining(),
            state.markdown_it().link_formatter.as_ref(),
        )?;

        let start = state.pos;

        let mut child = NodeDraft::new(TextSpecial {
            content: matched.label.clone(),
            markup: matched.label,
            info: "autolink",
        });
        child.set_srcmap(state.get_map(start + matched.label_start, start + matched.label_end));

        let mut container = NodeDraft::new(Autolink {
            url: matched.destination,
        });
        container.push_child(child);

        Some((Some(container), matched.consumed))
    }
}

fn scan_autolink(src: &str, formatter: &dyn LinkFormatter) -> Option<AutolinkMatch> {
    if !src.starts_with('<') {
        return None;
    }

    let mut closing = None;
    for (offset, ch) in src[1..].char_indices() {
        let offset = offset + 1;
        match ch {
            '<' => return None,
            '>' => {
                closing = Some(offset);
                break;
            }
            _ => {}
        }
    }

    let label_end = closing?;
    let raw = &src[1..label_end];

    let is_uri = AUTOLINK_RE.is_match(raw);
    let is_email = EMAIL_RE.is_match(raw);
    if !is_uri && !is_email {
        return None;
    }

    let destination = if is_uri {
        formatter.normalize_link(raw)
    } else {
        formatter.normalize_link(&format!("mailto:{raw}"))
    };
    formatter.validate_link(&destination)?;

    Some(AutolinkMatch {
        consumed: label_end + 1,
        label_start: 1,
        label_end,
        destination,
        label: formatter.normalize_link_text(raw),
    })
}

#[cfg(test)]
mod tests {
    use crate::MarkdownIt;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        crate::plugins::cmark::block::paragraph::add(&mut md);
        super::add(&mut md);
        md
    }

    fn render_html(md: &MarkdownIt, src: &str) -> String {
        let legacy = md.render(src);

        let bridged = md.parse_document(src);
        assert_eq!(legacy, md.render_document(&bridged).unwrap());

        let direct = md.parse_document_direct(src).unwrap();
        assert_eq!(legacy, md.render_document(&direct).unwrap());

        legacy
    }

    #[test]
    fn renders_autolinks() {
        let md = parser();
        let cases: &[(&str, &str)] = &[
            (
                "<http://foo.bar.baz>",
                "<p><a href=\"http://foo.bar.baz\">http://foo.bar.baz</a></p>",
            ),
            (
                "<https://foo.bar.baz/test?q=hello&id=22>",
                "<p><a href=\"https://foo.bar.baz/test?q=hello&amp;id=22\">https://foo.bar.baz/test?q=hello&amp;id=22</a></p>",
            ),
            (
                "<irc://foo.bar:2233/baz>",
                "<p><a href=\"irc://foo.bar:2233/baz\">irc://foo.bar:2233/baz</a></p>",
            ),
            (
                "<MAILTO:FOO@BAR.BAZ>",
                "<p><a href=\"MAILTO:FOO@BAR.BAZ\">MAILTO:FOO@BAR.BAZ</a></p>",
            ),
            (
                "<foo.bar@example.com>",
                "<p><a href=\"mailto:foo.bar@example.com\">foo.bar@example.com</a></p>",
            ),
            (
                "<foo+bar@example.com>",
                "<p><a href=\"mailto:foo+bar@example.com\">foo+bar@example.com</a></p>",
            ),
            (
                "<https://例子.example/path>",
                "<p><a href=\"https://xn--fsqu00a.example/path\">https://例子.example/path</a></p>",
            ),
            (
                "<javascript:alert(1)>",
                "<p>&lt;javascript:alert(1)&gt;</p>",
            ),
            ("<foo.bar.baz>", "<p>&lt;foo.bar.baz&gt;</p>"),
            ("<https://foo bar>", "<p>&lt;https://foo bar&gt;</p>"),
            (
                "<<https://example.com>",
                "<p>&lt;<a href=\"https://example.com\">https://example.com</a></p>",
            ),
            ("<https://example.com", "<p>&lt;https://example.com</p>"),
            (
                r"<https://example.com/\foo>",
                "<p><a href=\"https://example.com/%5Cfoo\">https://example.com/\\foo</a></p>",
            ),
        ];

        for (src, expected) in cases {
            assert_eq!(render_html(&md, src).trim(), *expected, "for {src:?}");
        }
    }

    #[test]
    fn direct_autolink_source_maps() {
        use crate::StructuralEvent;
        use crate::parser::inline::TextSpecial;

        let md = parser();
        let document = md
            .parse_document_direct("x <https://example.test> y")
            .unwrap();

        let spans: Vec<_> = document
            .events(document.root())
            .unwrap()
            .filter_map(|event| {
                if matches!(event, StructuralEvent::Exit(_)) {
                    return None;
                }

                let node = event.node();

                if node.is::<super::Autolink>() {
                    Some(("autolink", node.srcmap()?.get_byte_offsets()))
                } else if node
                    .cast::<TextSpecial>()
                    .is_some_and(|text| text.info == "autolink")
                {
                    Some(("label", node.srcmap()?.get_byte_offsets()))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(spans, vec![("autolink", (2, 24)), ("label", (3, 23)),],);
    }
}
