//! Highlight syntax (like `==this==`)

use crate::generics::inline::emph_pair;
use crate::{MarkdownIt, Node, NodeValue, Renderer};

#[derive(Debug)]
pub struct Mark;

impl NodeValue for Mark {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        fmt.open("mark", &node.attrs);
        fmt.contents(&node.children);
        fmt.close("mark");
    }
}

pub fn add(md: &mut MarkdownIt) {
    emph_pair::add_with::<'=', 2, true>(md, || Node::new(Mark));
}
