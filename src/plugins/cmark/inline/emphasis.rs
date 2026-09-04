//! Emphasis and strong emphasis
//!
//! looks like `*this*` or `__that__`
//!
//! <https://spec.commonmark.org/0.30/#emphasis-and-strong-emphasis>
use crate::generics::inline::emph_pair;
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;

#[derive(Debug)]
pub struct Em {
    pub marker: char,
}

struct EmDocumentRenderer;

impl DocumentNodeRenderer<Em> for EmDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &Em,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        render_inline_container(node, context, output, "em")
    }
}

impl NodeValue for Em {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("em", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("em");
    }
}

#[derive(Debug)]
pub struct Strong {
    pub marker: char,
}

struct StrongDocumentRenderer;

impl DocumentNodeRenderer<Strong> for StrongDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &Strong,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        render_inline_container(node, context, output, "strong")
    }
}

fn render_inline_container(
    node: NodeRef<'_>,
    context: &mut DocumentRenderContext<'_>,
    output: &mut crate::DocumentWriter,
    tag: &str,
) -> Result<(), DocumentRenderError> {
    write_html_open(output, tag, node.attrs())?;
    context.render_children(node.id(), output)?;
    write_html_close(output, tag)
}

impl NodeValue for Strong {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("strong", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("strong");
    }
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'*', 1, true>(md, || Node::new(Em { marker: '*' }));
    emph_pair::add_with::<'_', 1, false>(md, || Node::new(Em { marker: '_' }));
    emph_pair::add_with::<'*', 2, true>(md, || Node::new(Strong { marker: '*' }));
    emph_pair::add_with::<'_', 2, false>(md, || Node::new(Strong { marker: '_' }));
    md.add_document_renderer::<Em, _>("html", EmDocumentRenderer);
    md.add_document_renderer::<Strong, _>("html", StrongDocumentRenderer);
    md.add_document_renderer::<Em, _>("text", TransparentDocumentRenderer);
    md.add_document_renderer::<Strong, _>("text", TransparentDocumentRenderer);
}
