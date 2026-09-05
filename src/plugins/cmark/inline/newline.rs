//! Line breaks
//!
//! Processes EOL (`\n`, soft and hard breaks).
//!
//!  - <https://spec.commonmark.org/0.30/#hard-line-breaks>
//!  - <https://spec.commonmark.org/0.30/#soft-line-breaks>
use crate::parser::document::{NodeDraft, NodeRef};
use crate::parser::document_parser::DocumentInlineState;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    PlainTextBreakDocumentRenderer,
    write_html_self_close,
};
use crate::parser::inline::{DocumentInlineRule, InlineRule, InlineState};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

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
    ) -> Result<(), DocumentRenderError> {
        write_html_self_close(output, "br", &[], context.options().xhtml_out)?;
        context.cr(output)
    }
}

impl NodeValue for Hardbreak {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.self_close("br", &[]);
        fmt.cr();
    }
}

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
    ) -> Result<(), DocumentRenderError> {
        if context.options().breaks {
            write_html_self_close(output, "br", &[], context.options().xhtml_out)?;
        }
        context.cr(output)
    }
}

impl NodeValue for Softbreak {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.softbreak();
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.inline.add_rule::<NewlineScanner>();
    md.inline.add_document_rule::<NewlineScanner>();
    md.add_document_renderer::<Hardbreak, _>("html", HardbreakDocumentRenderer);
    md.add_document_renderer::<Softbreak, _>("html", SoftbreakDocumentRenderer);
    md.add_document_renderer::<Hardbreak, _>("text", PlainTextBreakDocumentRenderer);
    md.add_document_renderer::<Softbreak, _>("text", PlainTextBreakDocumentRenderer);
}

#[doc(hidden)]
pub struct NewlineScanner;

impl DocumentInlineRule for NewlineScanner {
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next()? != '\n' {
            return None;
        }
        let end = state.pos + 1 + chars.take_while(|ch| matches!(ch, ' ' | '\t')).count();
        let spaces = state
            .trailing_text()
            .bytes()
            .rev()
            .take_while(|&ch| ch == b' ')
            .count();
        state.pop_trailing_text(spaces);
        let node = if spaces >= 2 {
            NodeDraft::new(Hardbreak)
        } else {
            NodeDraft::new(Softbreak)
        };
        state.pos -= spaces;
        Some((Some(node), end - state.pos))
    }
}

impl InlineRule for NewlineScanner {
    const MARKER: char = '\n';
    const NAMES: &'static [&'static str] = &["newline"];

    fn check(state: &mut InlineState) -> Option<usize> {
        // check rule is required because run() modifies trailing text
        let mut chars = state.src[state.pos..state.pos_max].chars();
        if chars.next().unwrap() != '\n' {
            return None;
        }
        Some(1)
    }

    fn run(state: &mut InlineState) -> Option<(Node, usize)> {
        let mut chars = state.src[state.pos..state.pos_max].chars();

        if chars.next().unwrap() != '\n' {
            return None;
        }

        let mut pos = state.pos;
        pos += 1;

        // skip leading whitespaces from next line
        while let Some(' ' | '\t') = chars.next() {
            pos += 1;
        }

        // '  \n' -> hardbreak
        let mut tail_size = 0;
        let trailing_text = state.trailing_text_get();

        for ch in trailing_text.chars().rev() {
            if ch == ' ' {
                tail_size += 1;
            } else {
                break;
            }
        }

        state.trailing_text_pop(tail_size);

        let node = if tail_size >= 2 {
            Node::new(Hardbreak)
        } else {
            Node::new(Softbreak)
        };

        state.pos -= tail_size; // backtrack to include tail in source maps
        Some((node, pos - state.pos))
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
        let ast = parser().parse("hello\nworld");
        assert_eq!(ast.render(), "<p>hello\nworld</p>\n");
    }

    #[test]
    fn breaks_respects_xhtml_out() {
        let ast = parser().parse("hello\nworld");

        assert_eq!(
            ast.render_with(&RenderOptions {
                breaks: true,
                xhtml_out: true,
                ..Default::default()
            }),
            "<p>hello<br />\nworld</p>\n"
        );
    }
}
