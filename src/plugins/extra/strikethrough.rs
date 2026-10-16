//! Strikethrough syntax (like `~~this~~`)
use crate::MarkdownIt;
use crate::document::NodeValue;
use crate::parser::inline::helpers::emph_pair;
use crate::render::{HtmlInlineElementDocumentRenderer, TransparentDocumentRenderer};

#[derive(Debug)]
pub struct Strikethrough {
    pub marker: char,
}

impl NodeValue for Strikethrough {}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'~', 2, true>(md, |document| {
        document.create_node(Strikethrough { marker: '~' })
    });
    md.add_document_renderer::<Strikethrough, _>("html", HtmlInlineElementDocumentRenderer("s"));
    md.add_document_renderer::<Strikethrough, _>("text", TransparentDocumentRenderer);
}
