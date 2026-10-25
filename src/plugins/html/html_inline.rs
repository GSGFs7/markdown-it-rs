//! HTML inline syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#raw-html>
use super::anchor_delta;
use super::utils::regexps::HTML_TAG_RE;
use crate::MarkdownIt;
use crate::common::extset::InlineRootExtSet;
use crate::document::NodeRef;
use crate::parser::inline::{InlineRule, InlineState};
use crate::render::{NodeRenderer, RenderContext};

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<HtmlInlineScanner>();
    md.add_renderer::<HtmlInline, _>("html", HtmlInlineRenderer);
    md.add_renderer::<HtmlInline, _>("text", HtmlInlineRenderer);
}

#[derive(Debug, Default)]
struct HtmlInlineScanCache {
    no_comment_closer_range: Option<(usize, usize)>,
}

#[derive(Debug)]
pub struct HtmlInline {
    pub content: String,
}

struct HtmlInlineRenderer;

impl NodeRenderer<HtmlInline> for HtmlInlineRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlInline,
        _: &mut RenderContext<'_>,
        output: &mut crate::Writer,
    ) {
        output.write_str(&value.content);
    }
}

#[doc(hidden)]
pub struct HtmlInlineScanner;

impl InlineRule for HtmlInlineScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["html_inline"];

    fn check(context: &mut InlineState<'_>) -> Option<usize> {
        let matched = scan_html_inline(
            &context.src,
            context.pos,
            context.pos_max,
            &mut context.inline_ext,
        )?;
        context.link_level = context
            .link_level
            .checked_add(matched.link_level_delta)
            .expect("check link level overflow");
        Some(matched.consumed)
    }

    fn run(state: &mut crate::InlineState<'_>) -> Option<(Option<crate::NodeId>, usize)> {
        let matched =
            scan_html_inline(&state.src, state.pos, state.pos_max, &mut state.inline_ext)?;

        state.link_level += matched.link_level_delta;

        let (source, document) = state.remaining_and_document();

        Some((
            Some(document.create_node(HtmlInline {
                content: source[..matched.consumed].to_owned(),
            })),
            matched.consumed,
        ))
    }
}

struct HtmlInlineMatch {
    consumed: usize,
    link_level_delta: i32,
}

fn scan_html_inline(
    src: &str,
    pos: usize,
    pos_max: usize,
    inline_ext: &mut InlineRootExtSet,
) -> Option<HtmlInlineMatch> {
    let rest = &src[pos..pos_max];
    let mut chars = rest.chars();
    // Check start
    if chars.next()? != '<' {
        return None;
    }

    // Quick fail on second char
    let Some('!' | '?' | '/' | 'A'..='Z' | 'a'..='z') = chars.next() else {
        return None;
    };

    // this avoid complexity reach O(n^2)
    // <!--<!--<!--...-->...
    // ^^^^           ^^^
    //   |             |
    // only find there two, skip the middle part.
    if rest.starts_with("<!--") && !rest.starts_with("<!-->") && !rest.starts_with("<!--->") {
        let cached_miss = inline_ext
            .get::<HtmlInlineScanCache>()
            .and_then(|cache| cache.no_comment_closer_range)
            .is_some_and(|(start, end)| pos >= start && pos_max <= end);
        if cached_miss {
            return None;
        }

        if !rest.contains("-->") {
            inline_ext
                .get_or_insert_default::<HtmlInlineScanCache>()
                .no_comment_closer_range = Some((pos, pos_max));
            return None;
        }
    }

    let capture = HTML_TAG_RE.find(rest)?.as_str();

    Some(HtmlInlineMatch {
        consumed: capture.len(),
        link_level_delta: anchor_delta(capture),
    })
}

#[cfg(test)]
mod tests {
    fn render(input: &str) -> String {
        let md = &mut crate::MarkdownIt::empty();
        crate::plugins::cmark::add(md);
        crate::plugins::html::add(md);
        md.render(input)
    }

    #[test]
    fn comment_allows_internal_double_hyphens() {
        assert_eq!(
            render("foo <!-- this is a --\ncomment - with hyphens -->"),
            "<p>foo <!-- this is a --\ncomment - with hyphens --></p>\n",
        );
    }

    #[test]
    fn supports_short_comment_forms() {
        assert_eq!(
            render("foo <!--> foo -->\n\nfoo <!---> foo -->"),
            "<p>foo <!--> foo --&gt;</p>\n<p>foo <!---> foo --&gt;</p>\n",
        );
    }

    #[test]
    fn declaration_requires_only_an_initial_ascii_letter() {
        for declaration in [
            r"<!ELEMENc\T br EMPTY>",
            "<!bar baz>",
            "<!A>",
            "<!a>",
            "<!DOCTYPEhtml>",
            "<!a\\* &amp;\nbody>",
        ] {
            assert_eq!(
                render(&format!("foo {declaration}\n")),
                format!("<p>foo {declaration}</p>\n"),
            );
        }
    }

    #[test]
    fn declaration_rejects_non_ascii_initial_letters_and_missing_closers() {
        for (input, expected) in [
            ("foo <!>", "<p>foo &lt;!&gt;</p>\n"),
            ("foo <!1>", "<p>foo &lt;!1&gt;</p>\n"),
            ("foo <! A>", "<p>foo &lt;! A&gt;</p>\n"),
            ("foo <!é>", "<p>foo &lt;!é&gt;</p>\n"),
            ("foo <!a", "<p>foo &lt;!a</p>\n"),
            ("foo <!a> tail >", "<p>foo <!a> tail &gt;</p>\n"),
        ] {
            assert_eq!(render(input), expected);
        }
    }
}
