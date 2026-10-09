// Counts the crabs after inline parsing and appends a footer node.
use markdown_it::parser::core::{CoreRule, DocumentCoreRule};
use markdown_it::{MarkdownIt, NodeValue};

use super::block_rule::BlockFerris;
use super::inline_rule::InlineFerris;

#[derive(Debug)]
pub struct FerrisCounter(usize);
impl NodeValue for FerrisCounter {}

// This is an extension for the markdown parser.
struct FerrisCounterRule;
impl CoreRule for FerrisCounterRule {
    fn document_rule() -> DocumentCoreRule {
        // This is a custom function that will be invoked once per document.
        // It has the document as an argument and may modify its contents.
        DocumentCoreRule::FinalizeDocument(|root, _| {
            // walk through the arena tree and count the custom nodes
            // added by the other two rules
            let mut counter = 0;
            let mut stack = vec![root.root()];
            while let Some(node) = stack.pop() {
                if root.node(node).is::<InlineFerris>() || root.node(node).is::<BlockFerris>() {
                    counter += 1;
                }
                stack.extend_from_slice(root.children(node));
            }

            // append a counter to the root as a custom node
            let footer = root.create_node(FerrisCounter(counter));
            root.push_child(root.root(), footer);
        })
    }
}

struct CounterRenderer;
impl markdown_it::DocumentNodeRenderer<FerrisCounter> for CounterRenderer {
    fn render(
        &self,
        _: markdown_it::NodeRef<'_>,
        counter: &FerrisCounter,
        ctx: &mut markdown_it::DocumentRenderContext<'_>,
        out: &mut markdown_it::DocumentWriter,
    ) {
        ctx.cr(out);
        out.write_str("<footer class=\"ferris-counter\">");
        out.write_str(&match counter.0 {
            0 => "No crabs around here.".into(),
            1 => "There is a crab lurking in this document.".into(),
            n => format!("There are {n} crabs lurking in this document."),
        });
        out.write_str("</footer>");
        ctx.cr(out);
    }
}

// insert this rule into parser
pub fn add(md: &mut MarkdownIt) {
    md.add_rule::<FerrisCounterRule>().after_named("inline");
    md.add_document_renderer::<FerrisCounter, _>("html", CounterRenderer);
}
