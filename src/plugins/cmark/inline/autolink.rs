//! Autolinks
//!
//! `<https://example.org>`
//!
//! <https://spec.commonmark.org/0.30/#autolinks>
use std::sync::LazyLock;

use regex::Regex;

use crate::MarkdownIt;
use crate::document::{NodeRef, TextSpecial};
use crate::links::LinkFormatter;
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{NodeRenderer, RenderContext, TransparentRenderer, render_html_link};

#[derive(Debug)]
pub struct Autolink {
    pub url: String,
}

struct AutolinkRenderer;

impl NodeRenderer<Autolink> for AutolinkRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Autolink,
        context: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        render_html_link(node, &link.url, None, context, output);
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<AutolinkScanner>();
    md.add_renderer::<Autolink, _>("html", AutolinkRenderer);
    md.add_renderer::<Autolink, _>("text", TransparentRenderer);
}

static AUTOLINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([a-zA-Z][a-zA-Z0-9+.\-]{1,31}):([^<>\x00-\x20]*)$").unwrap());

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*)$").unwrap()
});

#[doc(hidden)]
pub struct AutolinkScanner;

struct AutolinkMatch {
    consumed: usize,
    #[cfg(feature = "sourcemap")]
    label_start: usize,
    #[cfg(feature = "sourcemap")]
    label_end: usize,
    destination: String,
    label: String,
}

impl InlineRule for AutolinkScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["autolink"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        match scan_autolink(
            context.remaining(),
            context.markdown_it().link_formatter.as_ref(),
        ) {
            Some(matched) => Some(matched.consumed),
            None => None,
        }
    }

    fn run(state: &mut crate::InlineState<'_>) -> Option<(Option<crate::NodeId>, usize)> {
        let matched = scan_autolink(
            state.remaining(),
            state.markdown_it().link_formatter.as_ref(),
        )?;

        #[cfg(feature = "sourcemap")]
        let start = state.pos;

        let child = state.document.create_node(TextSpecial {
            content: matched.label.clone(),
            markup: matched.label,
            info: "autolink",
        });

        #[cfg(feature = "sourcemap")]
        {
            let srcmap = state.get_map(start + matched.label_start, start + matched.label_end);
            state.document.node_mut(child).set_srcmap(srcmap);
        }

        let container = state.document.create_node(Autolink {
            url: matched.destination,
        });
        state.document.push_child(container, child);

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
        #[cfg(feature = "sourcemap")]
        label_start: 1,
        #[cfg(feature = "sourcemap")]
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
        md.render(src)
    }

    #[test]
    fn decodes_empty_punycode_label() {
        let md = parser();
        assert_eq!(
            md.render("<http://xn--.com/>\n"),
            "<p><a href=\"http://xn--.com/\">http://.com/</a></p>\n"
        );
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
    fn replaces_nul_before_matching_autolinks() {
        let md = parser();
        for (source, expected) in [
            (
                "<javascriript:ale:)b\x000>)>",
                "<p><a href=\"javascriript:ale:)b%EF%BF%BD0\">javascriript:ale:)b�0</a>)&gt;</p>\n",
            ),
            (
                "<https://example.com/a\0b>",
                "<p><a href=\"https://example.com/a%EF%BF%BDb\">https://example.com/a�b</a></p>\n",
            ),
            (
                "<javascript:ale\0rt(1)>",
                "<p>&lt;javascript:ale�rt(1)&gt;</p>\n",
            ),
            ("<a\0b@example.com>", "<p>&lt;a�b@example.com&gt;</p>\n"),
            (
                "<https://example.com/\0 x>",
                "<p>&lt;https://example.com/� x&gt;</p>\n",
            ),
        ] {
            assert_eq!(md.render(source), expected, "{source:?}");
        }
    }

    #[test]
    #[cfg(feature = "sourcemap")]
    fn nul_autolink_source_maps_use_normalized_byte_offsets() {
        let md = parser();
        let source = "雪 <ab:é\0x> 尾";
        let normalized = "雪 <ab:é\u{fffd}x> 尾";
        let document = md.parse(source);
        assert_eq!(document.source(), normalized);
        let start = normalized.find('<').unwrap();
        let end = normalized.find('>').unwrap();
        let mut found = false;
        for event in document.events(document.root()) {
            let node = event.node();
            if node.is::<super::Autolink>() {
                found = true;
                assert_eq!(node.srcmap().unwrap().get_byte_offsets(), (start, end + 1));
            } else if let Some(text) = node.cast::<crate::TextSpecial>() {
                assert_eq!(text.content, "ab:é�x");
                assert_eq!(node.srcmap().unwrap().get_byte_offsets(), (start + 1, end));
            }
        }
        assert!(found);
    }

    #[test]
    #[cfg(feature = "sourcemap")]
    fn direct_autolink_source_maps() {
        use crate::StructuralEvent;
        use crate::document::TextSpecial;

        let md = parser();
        let document = md.parse("x <https://example.test> y");

        let spans: Vec<_> = document
            .events(document.root())
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
