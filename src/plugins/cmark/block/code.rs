//! Indented code block
//!
//! Parses anything indented with 4 spaces.
//!
//! <https://spec.commonmark.org/0.30/#indented-code-block>
use std::fmt::Write;

use crate::parser::block::{BlockRule, BlockState};
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    write_html_close,
    write_html_open,
    write_html_text,
};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

const CODE_INDENT: i32 = 4;

#[derive(Debug)]
pub struct CodeBlock {
    pub content: String,
}

struct CodeBlockDocumentRenderer;

impl DocumentNodeRenderer<CodeBlock> for CodeBlockDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        code: &CodeBlock,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        context.cr(output)?;
        write_html_open(output, "pre", &[])?;
        write_html_open(output, "code", node.attrs())?;
        write_html_text(output, &code.content)?;
        write_html_close(output, "code")?;
        write_html_close(output, "pre")?;
        context.cr(output)
    }
}

struct CodeBlockTextRenderer;

impl DocumentNodeRenderer<CodeBlock> for CodeBlockTextRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        code: &CodeBlock,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        context.cr(output)?;
        output.write_str(&code.content)?;
        context.cr(output)
    }
}

impl NodeValue for CodeBlock {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.cr();
        fmt.open("pre", &[]);
        fmt.open("code", &node.attrs);
        fmt.text(&self.content);
        fmt.close("code");
        fmt.close("pre");
        fmt.cr();
    }
}

pub fn add(md: &mut MarkdownIt) {
    md.block.add_rule::<CodeScanner>();
    md.add_document_renderer::<CodeBlock, _>("html", CodeBlockDocumentRenderer);
    md.add_document_renderer::<CodeBlock, _>("text", CodeBlockTextRenderer);
    md.max_indent = CODE_INDENT;
}

#[doc(hidden)]
pub struct CodeScanner;
impl BlockRule for CodeScanner {
    const NAMES: &'static [&'static str] = &["code"];

    fn check(_: &mut BlockState) -> Option<()> {
        None
    }

    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        if state.line_indent(state.line) < CODE_INDENT {
            return None;
        }

        let mut next_line = state.line + 1;
        let mut last = next_line;

        while next_line < state.line_max {
            if state.is_empty(next_line) {
                next_line += 1;
                continue;
            }

            if state.line_indent(next_line) >= CODE_INDENT {
                next_line += 1;
                last = next_line;
                continue;
            }

            break;
        }

        let (mut content, _mapping) = state.get_lines(
            state.line,
            last,
            CODE_INDENT as usize + state.blk_indent,
            false,
        );
        content += "\n";

        let node = Node::new(CodeBlock { content });
        //node.srcmap = state.get_map_from_offsets(mapping[0].1, state.line_offsets[last - 1].line_end);

        Some((node, last - state.line))
    }
}
