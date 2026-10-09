//! HTML inline syntax from CommonMark
//!
//! <https://spec.commonmark.org/0.30/#raw-html>
use super::utils::regexps::*;
use crate::MarkdownIt;
use crate::common::extset::InlineRootExtSet;
use crate::document::{NodeRef, NodeValue};
use crate::parser::inline::{DocumentInlineState, InlineRule};
use crate::render::{DocumentNodeRenderer, DocumentRenderContext};

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<HtmlInlineScanner>();
    md.add_document_renderer::<HtmlInline, _>("html", HtmlInlineDocumentRenderer);
    md.add_document_renderer::<HtmlInline, _>("text", HtmlInlineTextRenderer);
}

#[derive(Debug, Default)]
struct HtmlInlineScanCache {
    no_comment_closer_range: Option<(usize, usize)>,
}

#[derive(Debug)]
pub struct HtmlInline {
    pub content: String,
}

struct HtmlInlineDocumentRenderer;

impl DocumentNodeRenderer<HtmlInline> for HtmlInlineDocumentRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlInline,
        _: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        output.write_str(&value.content);
    }
}

struct HtmlInlineTextRenderer;

impl DocumentNodeRenderer<HtmlInline> for HtmlInlineTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        value: &HtmlInline,
        _: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        output.write_str(&value.content);
    }
}

impl NodeValue for HtmlInline {}

#[doc(hidden)]
pub struct HtmlInlineScanner;

impl InlineRule for HtmlInlineScanner {
    const MARKER: char = '<';
    const NAMES: &'static [&'static str] = &["html_inline"];

    fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        let matched = scan_html_inline(
            &context.src,
            context.pos,
            context.pos_max,
            &mut context.inline_ext,
        );
        match matched {
            Some(matched) => {
                context.link_level = context
                    .link_level
                    .checked_add(matched.link_level_delta)
                    .expect("check link level overflow");
                Some(matched.consumed)
            }
            None => None,
        }
    }

    fn run(state: &mut crate::DocumentInlineState<'_>) -> Option<(Option<crate::NodeId>, usize)> {
        let matched =
            scan_html_inline(&state.src, state.pos, state.pos_max, &mut state.inline_ext)?;

        state.link_level += matched.link_level_delta;

        Some((
            Some(state.document.create_node(HtmlInline {
                content: matched.content,
            })),
            matched.consumed,
        ))
    }
}

struct HtmlInlineMatch {
    content: String,
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

    let capture = HTML_TAG_RE.captures(rest)?.get(0)?.as_str();
    let content = capture.to_owned();

    let link_level_delta = if HTML_LINK_OPEN.is_match(&content) {
        1
    } else if HTML_LINK_CLOSE.is_match(&content) {
        -1
    } else {
        0
    };

    Some(HtmlInlineMatch {
        content,
        consumed: capture.len(),
        link_level_delta,
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
}
