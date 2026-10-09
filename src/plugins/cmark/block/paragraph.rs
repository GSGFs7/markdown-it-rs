//! Paragraph
//!
//! This is the default rule if nothing else matches.
//!
//! <https://spec.commonmark.org/0.30/#paragraph>
use crate::MarkdownIt;
use crate::document::{NodeDraft, NodeValue};
use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::render::{HtmlBlockElementDocumentRenderer, PlainTextBlockDocumentRenderer};

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<ParagraphScanner>().after_all();
    md.document_renderers
        .add::<Paragraph, _>("html", HtmlBlockElementDocumentRenderer("p"));
    md.add_document_renderer::<Paragraph, _>("text", PlainTextBlockDocumentRenderer);
}

impl BlockRule for ParagraphScanner {
    const NAMES: &'static [&'static str] = &["paragraph"];

    fn check(_: &mut DocumentBlockState<'_>) -> Option<()> {
        None // can't interrupt anything
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        let start_line = state.line;
        let mut next_line = start_line;

        // jump line-by-line until an empty one or EOF
        loop {
            next_line += 1;
            if next_line >= state.line_max || state.is_empty(next_line) {
                break;
            }

            // this may be a code block normally, but after paragraph
            // it's considered a lazy continuation regardless of what's there
            if state.line_indent(next_line) >= state.md.max_indent {
                continue;
            }

            // quirk for blockquotes, this line should already be checked by that rule
            if state.line_offsets[next_line].indent_nonspace < 0 {
                continue;
            }

            // Some tags can terminate paragraph without empty line.
            let old_line = state.line;
            state.line = next_line;
            let interrupted = state.test_rules_at_line();
            state.line = old_line;
            if interrupted {
                break;
            }
        }

        let (content, mapping) = state.get_lines(start_line, next_line, state.blk_indent, false);
        let mut paragraph = NodeDraft::new(Paragraph);
        paragraph.push_child(state.pending_inline(content, mapping));
        Some((paragraph, next_line - start_line))
    }
}

#[derive(Debug)]
pub struct Paragraph;

impl NodeValue for Paragraph {}

#[doc(hidden)]
pub struct ParagraphScanner;
