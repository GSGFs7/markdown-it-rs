//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use crate::generics::inline::full_link;
use crate::parser::document::NodeRef;
use crate::parser::document_renderer::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    TransparentDocumentRenderer,
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
        attrs.push(("alt".into(), collect_document_alt_text(context, node)?));
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

// collect alt text, ignore marker
//
// e.g.
// raw: "![a *b* c](x)"
// children: [text "a ", emph(text "b"), text " c"]
// result: "a b c"
fn collect_document_alt_text(
    context: &DocumentRenderContext<'_>,
    image: NodeRef<'_>,
) -> Result<String, DocumentRenderError> {
    context.with_scratch_node_stack(|document, stack| {
        // reverse order in, original order out
        stack.extend(image.children().iter().rev().copied());
        let mut result = String::new();

        while let Some(id) = stack.pop() {
            let node = document.node(id)?;

            // pre-order dfs
            stack.extend(node.children().iter().rev().copied());

            append_document_alt_node(&mut result, node);
        }

        Ok(result)
    })
}

fn append_document_alt_node(result: &mut String, node: NodeRef<'_>) {
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

pub fn add(md: &mut MarkdownIt) {
    full_link::add_prefix::<'!', true>(md, |href, title| {
        Node::new(Image {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Image, _>("html", ImageDocumentRenderer);
    md.add_document_renderer::<Image, _>("text", TransparentDocumentRenderer);
}
