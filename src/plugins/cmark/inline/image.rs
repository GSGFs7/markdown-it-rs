//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use crate::MarkdownIt;
use crate::document::{NodeRef, NodeValue, Text, TextSpecial};
use crate::parser::inline::helpers::full_link;
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;
use crate::render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    TransparentDocumentRenderer,
    write_html_self_close,
};

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
    ) {
        let mut attrs = node.attrs().clone();
        attrs.push(("src".into(), image.url.clone()));
        attrs.push(("alt".into(), collect_document_alt_text(context, node)));
        if let Some(title) = &image.title {
            attrs.push(("title".into(), title.clone()));
        }
        write_html_self_close(output, "img", &attrs, context.options().xhtml_out);
    }
}

impl NodeValue for Image {}

// collect alt text, ignore marker
//
// e.g.
// raw: "![a *b* c](x)"
// children: [text "a ", emph(text "b"), text " c"]
// result: "a b c"
fn collect_document_alt_text(context: &DocumentRenderContext<'_>, image: NodeRef<'_>) -> String {
    context.with_scratch_node_stack(|document, stack| {
        // reverse order in, original order out
        stack.extend(image.children().iter().rev().copied());
        let mut result = String::new();

        while let Some(id) = stack.pop() {
            let node = document.node(id);

            // pre-order dfs
            stack.extend(node.children().iter().rev().copied());

            append_document_alt_node(&mut result, node);
        }

        result
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
    full_link::add_prefix::<'!', true>(md, |document, href, title| {
        document.create_node(Image {
            url: href.unwrap_or_default(),
            title,
        })
    });
    md.add_document_renderer::<Image, _>("html", ImageDocumentRenderer);
    md.add_document_renderer::<Image, _>("text", TransparentDocumentRenderer);
}

#[cfg(test)]
mod tests {
    #[test]
    fn alt_text_uses_a_fresh_inline_nesting_budget() {
        let mut md = crate::MarkdownIt::new();
        for limit in [3, 4, 5, 100] {
            md.max_nesting = limit;
            assert_eq!(
                md.render("![[[foo](uri1)](uri2)](uri3)"),
                "<p><img src=\"uri3\" alt=\"[foo](uri2)\"></p>\n"
            );
        }
        md.max_nesting = 2;
        assert_eq!(
            md.render("![![x](in)](out)"),
            "<p><img src=\"out\" alt=\"x\"></p>\n"
        );
    }
}
