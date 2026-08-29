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

#[cfg(test)]
mod tests {
    use markdown_it::MarkdownIt;

    use crate as markdown_it;

    fn run(input: &str, output: &str) {
        let md = &mut MarkdownIt::empty();
        markdown_it::plugins::cmark::add(md);
        markdown_it::plugins::extra::mark::add(md);
        markdown_it::plugins::extra::strikethrough::add(md);
        let html = md.parse(input).render();
        assert_eq!(html.trim(), output);
    }

    #[test]
    fn mark_simple() {
        run("==highlighted==", "<p><mark>highlighted</mark></p>");
    }

    #[test]
    fn mark_nested() {
        run(
            "==**bold** highlight==",
            "<p><mark><strong>bold</strong> highlight</mark></p>",
        );
    }

    #[test]
    fn mark_multiple() {
        run(
            "==one== and ==two==",
            "<p><mark>one</mark> and <mark>two</mark></p>",
        );
    }

    #[test]
    fn mark_mixed() {
        run(
            "==mark ~~strike~~==",
            "<p><mark>mark <s>strike</s></mark></p>",
        );
    }
}
