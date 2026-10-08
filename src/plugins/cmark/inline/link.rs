//! Links
//!
//! `![link](<to> "stuff")`
//!
//! <https://spec.commonmark.org/0.30/#links>
use crate::NodeDraft;
use crate::document::NodeRef;
use crate::generics::inline::full_link;
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
    ) {
        let mut attrs = node.attrs().clone();
        attrs.push(("href".into(), link.url.clone()));
        if let Some(title) = &link.title {
            attrs.push(("title".into(), title.clone()));
        }

        write_html_open(output, "a", &attrs);
        context.render_children(node.id(), output);
        write_html_close(output, "a");
    }
}

impl NodeValue for Link {}

pub fn add(md: &mut MarkdownIt) {
    full_link::add::<false>(md, |href, title| {
        NodeDraft::new(Link {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Link, _>("html", LinkDocumentRenderer);
    md.add_document_renderer::<Link, _>("text", TransparentDocumentRenderer);
}
