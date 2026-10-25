//! Strikethrough syntax (like `~~this~~`)
use crate::MarkdownIt;
use crate::parser::inline::helpers::emph_pair;
use crate::render::{HtmlInlineElementRenderer, TransparentRenderer};

#[derive(Debug)]
pub struct Strikethrough {
    pub marker: char,
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'~', 2, true>(md, |document| {
        document.create_node(Strikethrough { marker: '~' })
    });
    md.add_renderer::<Strikethrough, _>("html", HtmlInlineElementRenderer("s"));
    md.add_renderer::<Strikethrough, _>("text", TransparentRenderer);
}
