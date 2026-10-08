// Replaces `(\/)-------(\/)` with a nice picture.

use markdown_it::parser::block::BlockRule;
use markdown_it::{DocumentBlockState, MarkdownIt, NodeDraft, NodeValue};

const CRAB_CLAW: &str = r#"(\/)"#;
const CRAB_URL: &str = "https://upload.wikimedia.org/wikipedia/commons/0/0f/Original_Ferris.svg";

#[derive(Debug)]
// This is a structure that represents your custom node payload in the document.
pub struct BlockFerris;

impl NodeValue for BlockFerris {}

// This is an extension for the block subparser.
struct FerrisBlockScanner;

impl BlockRule for FerrisBlockScanner {
    // This is a custom function that will be invoked on every line
    // in a block context.
    //
    // It should get a line number `state.line` and report if your
    // custom structure appears there.
    //
    // If custom structure is found, it:
    //  - creates a new `NodeDraft`
    //  - increments `state.line` to a position after this node
    //  - returns true
    //
    // In "silent mode" (when `silent=true`) you aren't allowed to
    // create any nodes, should only increment `state.line`.
    //
    fn run(state: &mut DocumentBlockState) -> Option<(NodeDraft, usize)> {
        // get contents of a line number `state.line` and check it
        let line = state.get_line(state.line).trim();
        if !line.starts_with(CRAB_CLAW) {
            return None;
        }
        if !line.ends_with(CRAB_CLAW) {
            return None;
        }

        // require any number of `-` in between, but no less than 4
        if line.len() < CRAB_CLAW.len() * 2 + 4 {
            return None;
        }

        // and make sure no other characters are present there
        let dashes = &line[CRAB_CLAW.len()..line.len() - CRAB_CLAW.len()];
        if dashes.chars().any(|c| c != '-') {
            return None;
        }

        // return new node and number of lines it occupies
        Some((NodeDraft::new(BlockFerris), 1))
    }
}

pub fn add(md: &mut MarkdownIt) {
    // insert this rule into block subparser
    md.block.add_rule::<FerrisBlockScanner>();
    md.add_document_renderer::<BlockFerris, _>("html", BlockFerrisRenderer);
}

// This defines how your custom node should be rendered.
struct BlockFerrisRenderer;
impl markdown_it::DocumentNodeRenderer<BlockFerris> for BlockFerrisRenderer {
    fn render(
        &self,
        _: markdown_it::NodeRef<'_>,
        _: &BlockFerris,
        ctx: &mut markdown_it::DocumentRenderContext<'_>,
        out: &mut markdown_it::DocumentWriter,
    ) {
        ctx.cr(out);
        out.write_str("<div class=\"ferris-block\"><img src=\"");
        out.write_str(CRAB_URL);
        out.write_str(if ctx.options().xhtml_out {
            "\" /></div>"
        } else {
            "\"></div>"
        });
        ctx.cr(out);
    }
}
