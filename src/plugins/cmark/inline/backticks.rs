//! Code spans
//!
//! `` `looks like this` ``
//!
//! <https://spec.commonmark.org/0.30/#code-span>
use crate::generics::inline::code_pair;
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    write_html_close,
    write_html_open,
};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

#[derive(Debug)]
pub struct CodeInline {
    pub marker: char,
    pub marker_len: usize,
}

struct CodeInlineDocumentRenderer;

impl DocumentNodeRenderer<CodeInline> for CodeInlineDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &CodeInline,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        write_html_open(output, "code", node.attrs())?;
        context.render_children(node.id(), output)?;
        write_html_close(output, "code")
    }
}

impl NodeValue for CodeInline {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("code", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("code");
    }
}

pub fn add(md: &mut MarkdownIt) {
    code_pair::add_with::<'`'>(md, |len| {
        Node::new(CodeInline {
            marker: '`',
            marker_len: len,
        })
    });
    md.add_document_renderer::<CodeInline, _>("html", CodeInlineDocumentRenderer);
}
