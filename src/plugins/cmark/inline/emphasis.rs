//! Emphasis and strong emphasis
//!
//! looks like `*this*` or `__that__`
//!
//! <https://spec.commonmark.org/0.30/#emphasis-and-strong-emphasis>
use crate::MarkdownIt;
use crate::parser::inline::helpers::emph_pair;
use crate::render::{HtmlInlineElementRenderer, TransparentRenderer};

#[derive(Debug)]
pub struct Em {
    pub marker: char,
}

#[derive(Debug)]
pub struct Strong {
    pub marker: char,
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'*', 1, true>(md, |document| document.create_node(Em { marker: '*' }));
    emph_pair::add_with::<'_', 1, false>(md, |document| document.create_node(Em { marker: '_' }));
    emph_pair::add_with::<'*', 2, true>(md, |document| {
        document.create_node(Strong { marker: '*' })
    });
    emph_pair::add_with::<'_', 2, false>(md, |document| {
        document.create_node(Strong { marker: '_' })
    });
    md.add_renderer::<Em, _>("html", HtmlInlineElementRenderer("em"));
    md.add_renderer::<Strong, _>("html", HtmlInlineElementRenderer("strong"));
    md.add_renderer::<Em, _>("text", TransparentRenderer);
    md.add_renderer::<Strong, _>("text", TransparentRenderer);
}
