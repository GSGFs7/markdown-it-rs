//! Strikethrough syntax (like `~~this~~`)
use crate::NodeDraft;
use crate::document::NodeRef;
use crate::generics::inline::emph_pair;
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
        output: &mut crate::DocumentWriter,
    ) {
        write_html_open(output, "s", node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, "s");
    }
}

impl NodeValue for Strikethrough {}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'~', 2, true>(md, || NodeDraft::new(Strikethrough { marker: '~' }));
    md.add_document_renderer::<Strikethrough, _>("html", StrikethroughDocumentRenderer);
    md.add_document_renderer::<Strikethrough, _>("text", TransparentDocumentRenderer);
}
