//! Images
//!
//! `![image](<src> "title")`
//!
//! <https://spec.commonmark.org/0.30/#images>
use crate::generics::inline::full_link;
use crate::parser::inline::{Text, TextSpecial};
use crate::plugins::cmark::inline::newline::{Hardbreak, Softbreak};
use crate::plugins::html::html_inline::HtmlInline;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Image {
    pub url: String,
    pub title: Option<String>,
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

pub fn add(md: &mut MarkdownIt) {
    full_link::add_prefix::<'!', true>(md, |href, title| {
        Node::new(Image {
            url: href.unwrap_or_default(),
            title,
        })
    });
}
