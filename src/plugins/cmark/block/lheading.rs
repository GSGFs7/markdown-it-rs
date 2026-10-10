//! Setext headings
//!
//! Paragraph underlined with `===` or `---`.
//!
//! <https://spec.commonmark.org/0.30/#setext-headings>
use crate::MarkdownIt;
use crate::document::{NodeId, NodeRef, NodeValue};
use crate::parser::block::{BlockRule, DocumentBlockState};
use crate::plugins::cmark::block::paragraph::{ParagraphScanner, test_paragraph_rules_at_line};
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
};

#[derive(Debug)]
pub struct SetextHeader {
    pub level: u8,
    pub marker: char,
}

struct SetextHeaderDocumentRenderer;

impl DocumentNodeRenderer<SetextHeader> for SetextHeaderDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        heading: &SetextHeader,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        static TAG: [&str; 2] = ["h1", "h2"];
        debug_assert!((1..=2).contains(&heading.level));

        let tag = TAG[heading.level as usize - 1];

        context.cr(output);
        write_html_open(output, tag, node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, tag);
        context.cr(output);
    }
}

impl NodeValue for SetextHeader {}

pub fn add(md: &mut MarkdownIt) {
    md.block
        .add_rule::<LHeadingScanner>()
        .before::<ParagraphScanner>()
        .after_all();
    md.add_document_renderer::<SetextHeader, _>("html", SetextHeaderDocumentRenderer);
    md.add_document_renderer::<SetextHeader, _>("text", PlainTextBlockDocumentRenderer);
}

#[doc(hidden)]
pub struct LHeadingScanner;

/// Recognize a setext underline line, returning the heading level.
fn scan_setext_underline(line: &str) -> Option<u8> {
    let mut chars = line.chars().peekable();
    let marker @ ('-' | '=') = chars.next()? else {
        return None;
    };

    while Some(&marker) == chars.peek() {
        chars.next();
    }
    while let Some(' ' | '\t') = chars.peek() {
        chars.next();
    }

    if chars.next().is_none() {
        Some(if marker == '=' { 1 } else { 2 })
    } else {
        None
    }
}

impl BlockRule for LHeadingScanner {
    // no `MARKERS` here on purpose
    const NAMES: &'static [&'static str] = &["lheading"];

    fn check(_: &mut DocumentBlockState<'_>) -> Option<()> {
        None // can't interrupt any tags
    }

    fn run(state: &mut DocumentBlockState<'_>) -> Option<(Option<NodeId>, usize)> {
        if state.line_indent(state.line) >= state.md.max_indent {
            return None;
        }

        let start_line = state.line;
        let mut next_line = start_line;
        let mut level = 0;

        'outer: loop {
            next_line += 1;

            if next_line >= state.line_max || state.is_empty(next_line) {
                break;
            }

            // this may be a code block normally, but after paragraph
            // it's considered a lazy continuation regardless of what's there
            if state.line_indent(next_line) >= state.md.max_indent {
                continue;
            }

            //
            // check for underline in setext header
            //
            if state.line_indent(next_line) >= 0 {
                if let Some(underline_level) = scan_setext_underline(state.get_line(next_line)) {
                    level = underline_level;
                    break 'outer;
                }
            }

            // quirk for blockquotes, this line should already be checked by that rule
            if state.line_offsets[next_line].indent_nonspace < 0 {
                continue;
            }

            // Some tags can terminate paragraph without empty line.
            let old_state_line = state.line;
            state.line = next_line;
            let interrupted = test_paragraph_rules_at_line(state);
            state.line = old_state_line;
            if interrupted {
                break 'outer;
            }
        }

        if level == 0 {
            // Didn't find valid underline
            return None;
        }

        let (content, mapping) = state.get_lines(start_line, next_line, state.blk_indent, false);

        let node = state.document.create_node(SetextHeader {
            level,
            marker: if level == 2 { '-' } else { '=' },
        });
        let pending = state.pending_inline(content, mapping);
        state.document.push_child(node, pending);

        Some((Some(node), next_line + 1 - start_line))
    }
}
