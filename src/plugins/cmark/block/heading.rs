//! ATX heading
//!
//! `# h1`, `## h2`, etc.
//!
//! <https://spec.commonmark.org/0.30/#atx-heading>
use crate::document::{NodeDraft, NodeRef};
use crate::parser::block::BlockRule;
use crate::parser::document_parser::DocumentBlockState;
use crate::parser::main::MarkdownIt;
use crate::parser::node::NodeValue;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct ATXHeading {
    pub level: u8,
}

struct ATXHeadingDocumentRenderer;

impl DocumentNodeRenderer<ATXHeading> for ATXHeadingDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        heading: &ATXHeading,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        static TAG: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];
        debug_assert!((1..=6).contains(&heading.level));
        let tag = TAG[heading.level as usize - 1];

        context.cr(output);
        write_html_open(output, tag, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, tag);
        context.cr(output);
    }
}

impl NodeValue for ATXHeading {}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<HeadingScanner>();
    md.add_document_renderer::<ATXHeading, _>("html", ATXHeadingDocumentRenderer);
    md.add_document_renderer::<ATXHeading, _>("text", PlainTextBlockDocumentRenderer);
}

#[doc(hidden)]
pub struct HeadingScanner;

/// Scan an ATX heading line, returning `(level, text_pos, text_max)`.
fn scan_atx_heading(line: &str) -> Option<(u8, usize, usize)> {
    let Some('#') = line.chars().next() else {
        return None;
    };

    let text_pos;

    // count heading level
    let mut level = 0u8;
    let mut chars = line.char_indices();
    loop {
        match chars.next() {
            Some((_, '#')) => {
                level += 1;
                if level > 6 {
                    return None;
                }
            }
            Some((x, ' ' | '\t')) => {
                text_pos = x;
                break;
            }
            None => {
                text_pos = level as usize;
                break;
            }
            Some(_) => return None,
        }
    }

    // Let's cut tails like '    ###  ' from the end of string
    let mut chars_back = chars.rev().peekable();
    while let Some((_, ' ' | '\t')) = chars_back.peek() {
        chars_back.next();
    }
    while let Some((_, '#')) = chars_back.peek() {
        chars_back.next();
    }

    let text_max = match chars_back.next() {
        // ## foo ##
        Some((last_pos, ' ' | '\t')) => last_pos + 1,
        // ## foo##
        Some(_) => line.len(),
        // ## ## (already consumed the space)
        None => text_pos,
    };

    Some((level, text_pos, text_max))
}

impl BlockRule for HeadingScanner {
    const MARKERS: &'static [char] = &['#'];
    const NAMES: &'static [&'static str] = &["heading"];

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(NodeDraft, usize)> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let line = state.get_line(state.line);
        let (level, text_pos, text_max) = scan_atx_heading(line)?;

        let content = line[text_pos..text_max].to_owned();
        let mapping = vec![(0, state.line_offsets[state.line].first_nonspace + text_pos)];

        let mut node = NodeDraft::new(ATXHeading { level });
        node.push_child(state.pending_inline(content, mapping));
        Some((node, 1))
    }
}
