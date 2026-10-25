// Counts the crabs after inline parsing and appends a footer node.
use markdown_it::MarkdownIt;
use markdown_it::parser::core::{CoreRule, CoreState};

use super::block_rule::BlockFerris;
use super::inline_rule::InlineFerris;

#[derive(Debug)]
pub struct FerrisCounter(usize);

// This is an extension for the markdown parser.
#[derive(Default)]
struct FerrisCounterRule;
impl CoreRule for FerrisCounterRule {
    fn run(&self, state: &mut CoreState<'_>) {
        // Count custom nodes added by the other two rules.
        let root = &mut *state.document;
        let mut counter = 0;
        let mut stack = vec![root.root()];
        while let Some(node) = stack.pop() {
            if root.node(node).is::<InlineFerris>() || root.node(node).is::<BlockFerris>() {
                counter += 1;
            }
            stack.extend_from_slice(root.children(node));
        }

        // Append the count to the root.
        let footer = root.create_node(FerrisCounter(counter));
        root.push_child(root.root(), footer);
    }
}

struct CounterRenderer;
impl markdown_it::NodeRenderer<FerrisCounter> for CounterRenderer {
    fn render(
        &self,
        _: markdown_it::NodeRef<'_>,
        counter: &FerrisCounter,
        ctx: &mut markdown_it::RenderContext<'_>,
        out: &mut markdown_it::Writer,
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
    md.add_renderer::<FerrisCounter, _>("html", CounterRenderer);
}
