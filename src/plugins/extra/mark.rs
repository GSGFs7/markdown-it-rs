//! Highlight syntax (like `==this==`)

use crate::document::NodeRef;
use crate::parser::inline::helpers::emph_pair;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_close,
    write_html_open,
};
use crate::{MarkdownIt, NodeValue};

#[derive(Debug)]
pub struct Mark;

struct MarkDocumentRenderer;

impl DocumentNodeRenderer<Mark> for MarkDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        _: &Mark,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) {
        write_html_open(output, "mark", node.attrs());
        context.render_children(node.id(), output);
        write_html_close(output, "mark");
    }
}

impl NodeValue for Mark {}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'=', 2, true>(md, |document| document.create_node(Mark));
    md.add_document_renderer::<Mark, _>("html", MarkDocumentRenderer);
    md.add_document_renderer::<Mark, _>("text", TransparentDocumentRenderer);
}

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;

    use crate as markdown_it;

    fn run(input: &str, output: &str) {
        let md = &mut MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::mark::add(md);
        markdown_it::plugins::extra::strikethrough::add(md);
        let html = md.render(input);
        assert_eq!(html.trim(), output);
    }

    #[test]
    fn mark_simple() {
        run("==highlighted==", "<p><mark>highlighted</mark></p>");
    }

    #[test]
    fn mark_nested() {
        run(
            "==**bold** highlight==",
            "<p><mark><strong>bold</strong> highlight</mark></p>",
        );
    }

    #[test]
    fn mark_multiple() {
        run(
            "==one== and ==two==",
            "<p><mark>one</mark> and <mark>two</mark></p>",
        );
    }

    #[test]
    fn mark_mixed() {
        run(
            "==mark ~~strike~~==",
            "<p><mark>mark <s>strike</s></mark></p>",
        );
    }
}
