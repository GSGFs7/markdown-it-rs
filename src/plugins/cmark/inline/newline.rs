//! Line breaks
//!
//! Processes EOL (`\n`, soft and hard breaks).
//!
//!  - <https://spec.commonmark.org/0.30/#hard-line-breaks>
//!  - <https://spec.commonmark.org/0.30/#soft-line-breaks>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef, NodeValue};
use crate::parser::inline::{DocumentInlineState, InlineRule};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBreakDocumentRenderer,
    write_html_self_close,
};

#[derive(Debug)]
pub struct Hardbreak;

struct HardbreakDocumentRenderer;

impl DocumentNodeRenderer<Hardbreak> for HardbreakDocumentRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &Hardbreak,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        write_html_self_close(output, "br", &[], context.options().xhtml_out);
        context.cr(output);
    }
}

impl NodeValue for Hardbreak {}

#[derive(Debug)]
pub struct Softbreak;

struct SoftbreakDocumentRenderer;

impl DocumentNodeRenderer<Softbreak> for SoftbreakDocumentRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &Softbreak,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        if context.options().breaks {
            write_html_self_close(output, "br", &[], context.options().xhtml_out);
        }
        context.cr(output);
    }
}

impl NodeValue for Softbreak {}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<NewlineScanner>();
    md.add_document_renderer::<Hardbreak, _>("html", HardbreakDocumentRenderer);
    md.add_document_renderer::<Softbreak, _>("html", SoftbreakDocumentRenderer);
    md.add_document_renderer::<Hardbreak, _>("text", PlainTextBreakDocumentRenderer);
    md.add_document_renderer::<Softbreak, _>("text", PlainTextBreakDocumentRenderer);
}

#[doc(hidden)]
pub struct NewlineScanner;

impl InlineRule for NewlineScanner {
    const MARKER: char = '\n';
    const NAMES: &'static [&'static str] = &["newline"];

    fn check(context: &mut DocumentInlineState<'_>) -> Option<usize> {
        // check rule is required because run() modifies trailing text
        if context.remaining().starts_with('\n') {
            Some(1)
        } else {
            None
        }
    }

    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeId>, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next()? != '\n' {
            return None;
        }

        // skip leading whitespaces from next line
        let end = state.pos + 1 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count();
        let spaces = state
            .trailing_text()
            .bytes()
            .rev()
            .take_while(|&ch| ch == b' ')
            .count();
        state.pop_trailing_text(spaces);

        // '  \n' -> hardbreak
        let node = if spaces >= 2 {
            state.document.create_node(Hardbreak)
        } else {
            state.document.create_node(Softbreak)
        };
        state.pos -= spaces; // backtrack to include tail in source maps
        Some((Some(node), end - state.pos))
    }
}

#[cfg(test)]
mod test {
    use crate::*;

    fn parser() -> MarkdownIt {
        let mut md = MarkdownIt::empty();
        plugins::cmark::add(&mut md);
        md
    }

    #[test]
    fn renders_softbreak_as_newline_by_default() {
        let md = parser();
        let ast = md.parse_document("hello\nworld");
        assert_eq!(md.render_document(&ast), "<p>hello\nworld</p>\n");
    }

    #[test]
    fn breaks_respects_xhtml_out() {
        let md = parser();
        let ast = md.parse_document("hello\nworld");

        assert_eq!(
            md.document_renderers.render(
                &ast,
                "html",
                &RenderOptions {
                    breaks: true,
                    xhtml_out: true,
                    ..Default::default()
                }
            ),
            "<p>hello<br />\nworld</p>\n"
        );
    }

    #[test]
    fn document_breaks_match_expected_output() {
        let mut md = MarkdownIt::empty();
        plugins::cmark::block::paragraph::add(&mut md);
        plugins::cmark::inline::newline::add(&mut md);

        for (source, expected) in [
            ("a  \n \tb", "<p>a<br>\nb</p>\n"),
            ("a\nb", "<p>a\nb</p>\n"),
            ("a  \nb", "<p>a<br>\nb</p>\n"),
        ] {
            assert_eq!(md.render(source), expected, "{source}");
        }
    }
}
