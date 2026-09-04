//! Links
//!
//! `![link](<to> "stuff")`
//!
//! <https://spec.commonmark.org/0.30/#links>
use crate::generics::inline::full_link;
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
pub struct Link {
    pub url: String,
    pub title: Option<String>,
}

struct LinkDocumentRenderer;

impl DocumentNodeRenderer<Link> for LinkDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        link: &Link,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        if let Some(title) = &link.title {
            attrs.push(("title".into(), title.clone()));
        }

        write_html_open(output, "a", &attrs)?;
        context.render_children(node.id(), output)?;
        write_html_close(output, "a")
    }
}

impl NodeValue for Link {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("href".into(), self.url.clone()));

        if let Some(title) = &self.title {
            attrs.push(("title".into(), title.clone()));
        }

        fmt.open("a", &attrs);
        fmt.contents(&node.children);
        fmt.close("a");
    }
}

pub fn add(md: &mut MarkdownIt) {
    full_link::add::<false>(md, |href, title| {
        Node::new(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Link, _>("html", LinkDocumentRenderer);
    md.add_document_renderer::<Link, _>("text", TransparentDocumentRenderer);
}
