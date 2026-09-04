//! Setext headings
//!
//! Paragraph underlined with `===` or `---`.
//!
//! <https://spec.commonmark.org/0.30/#setext-headings>
use crate::parser::block::{BlockRule, BlockState};
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    PlainTextBlockDocumentRenderer,
    write_html_close,
    write_html_open,
};
use crate::parser::inline::InlineRoot;
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;
use crate::plugins::cmark::block::paragraph::ParagraphScanner;

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
    ) -> Result<(), DocumentRenderError> {
        static TAG: [&str; 2] = ["h1", "h2"];
        debug_assert!((1..=2).contains(&heading.level));
        let tag = TAG[heading.level as usize - 1];

        context.cr(output)?;
        write_html_open(output, tag, node.attrs())?;
        context.render_children(node.id(), output)?;
        write_html_close(output, tag)?;
        context.cr(output)
    }
}

impl NodeValue for SetextHeader {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        static TAG: [&str; 2] = ["h1", "h2"];
        debug_assert!(self.level >= 1 && self.level <= 2);

        fmt.cr();
        fmt.open(TAG[self.level as usize - 1], &node.attrs);
        fmt.contents(&node.children);
        fmt.close(TAG[self.level as usize - 1]);
        fmt.cr();
    }
}

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
impl BlockRule for LHeadingScanner {
    // no `MARKERS` here on purpose
    const NAMES: &'static [&'static str] = &["lheading"];

    fn check(_: &mut BlockState) -> Option<()> {
        None // can't interrupt any tags
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
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
            // Check for underline in setext header
            //
            if state.line_indent(next_line) >= 0 {
                let mut chars = state.get_line(next_line).chars().peekable();
                if let Some(marker @ ('-' | '=')) = chars.next() {
                    while Some(&marker) == chars.peek() {
                        chars.next();
                    }
                    while let Some(' ' | '\t') = chars.peek() {
                        chars.next();
                    }
                    if chars.next().is_none() {
                        level = if marker == '=' { 1 } else { 2 };
                        break 'outer;
                    }
                }
            }

            // quirk for blockquotes, this line should already be checked by that rule
            if state.line_offsets[next_line].indent_nonspace < 0 {
                continue;
            }

            // Some tags can terminate paragraph without empty line.
            let old_state_line = state.line;
            state.line = next_line;
            if state.test_rules_at_line() {
                state.line = old_state_line;
                break 'outer;
            }
            state.line = old_state_line;
        }

        if level == 0 {
            // Didn't find valid underline
            return None;
        }

        let (content, mapping) = state.get_lines(start_line, next_line, state.blk_indent, false);

        let mut node = Node::new(SetextHeader {
            level,
            marker: if level == 2 { '-' } else { '=' },
        });
        node.children
            .push(Node::new(InlineRoot::new(content, mapping)));

        Some((node, next_line + 1 - start_line))
    }
}
