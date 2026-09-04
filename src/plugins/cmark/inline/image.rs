//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use crate::generics::inline::full_link;
use crate::parser::document::{Document, NodeId, NodeRef, StructuralEvent};
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    write_html_self_close,
};
use crate::parser::inline::{Text, TextSpecial};
use crate::parser::main::MarkdownIt;
use crate::parser::node::{Node, NodeValue};
use crate::parser::renderer::Renderer;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;

#[derive(Debug)]
pub struct Image {
    pub url: String,
    pub title: Option<String>,
}

struct ImageDocumentRenderer;

impl DocumentNodeRenderer<Image> for ImageDocumentRenderer {
    fn render(
        &self,
        node: NodeRef<'_>,
        image: &Image,
        context: &mut DocumentRenderContext<'_>,
        output: &mut crate::DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        let mut attrs = node.attrs().clone();
        attrs.push(("src".into(), image.url.clone()));
        attrs.push((
            "alt".into(),
            collect_document_alt_text(context.document(), node.id())?,
        ));
        if let Some(title) = &image.title {
            attrs.push(("title".into(), title.clone()));
        }
        write_html_self_close(output, "img", &attrs, context.options().xhtml_out)
    }
}

impl NodeValue for Image {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        let mut attrs = node.attrs.clone();
        attrs.push(("src".into(), self.url.clone()));
        attrs.push(("alt".into(), collect_alt_text(node)));

        if let Some(title) = &self.title {
            attrs.push(("title".into(), title.clone()));
        }

        fmt.self_close("img", &attrs);
    }
}

fn collect_alt_text(node: &Node) -> String {
    let mut result = String::new();

    node.walk(|node, _| {
        if let Some(text) = node.cast::<Text>() {
            result.push_str(&text.content);
        } else if let Some(text) = node.cast::<TextSpecial>() {
            result.push_str(&text.content);
        } else if let Some(html) = node.cast::<HtmlInline>() {
            result.push_str(&html.content);
        } else if node.is::<Softbreak>() || node.is::<Hardbreak>() {
            result.push('\n');
        }
    });

    result
}

fn collect_document_alt_text(
    document: &Document,
    image: NodeId,
) -> Result<String, DocumentRenderError> {
    let mut result = String::new();
    for event in document.events(image)? {
        let node = match event {
            StructuralEvent::Enter(node) | StructuralEvent::Leaf(node) => node,
            StructuralEvent::Exit(_) => continue,
        };
        if let Some(text) = node.cast::<Text>() {
            result.push_str(&text.content);
        } else if let Some(text) = node.cast::<TextSpecial>() {
            result.push_str(&text.content);
        } else if let Some(html) = node.cast::<HtmlInline>() {
            result.push_str(&html.content);
        } else if node.is::<Softbreak>() || node.is::<Hardbreak>() {
            result.push('\n');
        }
    }
    Ok(result)
}

pub fn add(md: &mut MarkdownIt) {
    full_link::add_prefix::<'!', true>(md, |href, title| {
        Node::new(Image {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Image, _>("html", ImageDocumentRenderer);
}
