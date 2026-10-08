//! Code spans
//!
//! `` `looks like this` ``
//!
//! <https://spec.commonmark.org/0.30/#code-span>
use crate::document::{NodeDraft, NodeRef};
use crate::generics::inline::code_pair;
use crate::parser::main::MarkdownIt;
use crate::parser::node::NodeValue;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};

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
    ) {
        write_html_open(output, "code", node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, "code");
    }
}

impl NodeValue for CodeInline {}

pub fn add(md: &mut MarkdownIt) {
    code_pair::add_with::<'`'>(md, |len| {
        NodeDraft::new(CodeInline {
            marker: '`',
            marker_len: len,
        })
    });
    md.add_document_renderer::<CodeInline, _>("html", CodeInlineDocumentRenderer);
    md.add_document_renderer::<CodeInline, _>("text", TransparentDocumentRenderer);
}
