//! Paragraph
//!
//! This is the default rule if nothing else matches.
//!
//! <https://spec.commonmark.org/0.30/#paragraph>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeValue};
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

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
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
            let interrupted = test_paragraph_rules_at_line(state);
            state.line = old_line;
            if interrupted {
                break;
            }
        }

        let (content, mapping) = state.get_lines(start_line, next_line, state.blk_indent, false);
        let paragraph = state.document.create_node(Paragraph);
        let pending = state.pending_inline(content, mapping);
        state.document.push_child(paragraph, pending);
        Some((Some(paragraph), next_line - start_line))
    }
}

#[derive(Debug)]
pub struct Paragraph;

impl NodeValue for Paragraph {}

#[doc(hidden)]
pub struct ParagraphScanner;

/// Temporary plugin context: list interruption restrictions apply only to paragraphs.
#[derive(Debug)]
pub(super) struct ParagraphInterrupt;

pub(super) fn test_paragraph_rules_at_line(state: &mut DocumentBlockState<'_>) -> bool {
    let old_context = state.root_ext.insert(ParagraphInterrupt);
    let interrupted = state.test_rules_at_line();
    if let Some(old_context) = old_context {
        state.root_ext.insert(old_context);
    } else {
        state.root_ext.remove::<ParagraphInterrupt>();
    }
    interrupted
}
