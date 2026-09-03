//! Strikethrough syntax (like `~~this~~`)
use crate::generics::inline::emph_pair;
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
pub struct Strikethrough {
    pub marker: char,
}

struct StrikethroughDocumentRenderer;

impl DocumentNodeRenderer<Strikethrough> for StrikethroughDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &Strikethrough,
        context: &mut DocumentRenderContext<'_>,
        output: &mut dyn std::fmt::Write,
    ) -> Result<(), DocumentRenderError> {
        write_html_open(output, "s", node.attrs())?;
        context.render_children(node.id(), output)?;
        write_html_close(output, "s")
    }
}

impl NodeValue for Strikethrough {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("s", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("s");
    }
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'~', 2, true>(md, || Node::new(Strikethrough { marker: '~' }));
    md.add_document_renderer::<Strikethrough, _>("html", StrikethroughDocumentRenderer);
}
