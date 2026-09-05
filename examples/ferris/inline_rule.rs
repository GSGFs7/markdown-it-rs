// Replaces `(\/)` with `🦀`.

use std::fmt::Write;

use markdown_it::parser::inline::InlineRule;
use markdown_it::{
    DocumentInlineState,
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRenderError,
    DocumentWriter,
    MarkdownIt,
    Node,
    NodeDraft,
    NodeRef,
    NodeValue,
    Renderer,
};

const CRAB_CLAW: &str = r#"(\/)"#;

#[derive(Debug)]
// This is a structure that represents your custom Node in AST.
pub struct InlineFerris;

// This defines how your custom node should be rendered.
impl NodeValue for InlineFerris {
    fn render(&self, node: &Node, fmt: &mut dyn Renderer) {
        // `node.attrs` are custom attributes added by other plugins
        // (for example, source mapping information)
        let mut attrs = node.attrs.clone();

        // add a custom class attribute
        attrs.push(("class".into(), "ferris-inline".into()));

        fmt.open("span", &attrs);
        fmt.text("🦀");
        fmt.close("span");
    }
}

struct InlineFerrisDocumentRenderer;

impl DocumentNodeRenderer<InlineFerris> for InlineFerrisDocumentRenderer {
    fn render(
        &self,
        _: NodeRef<'_>,
        _: &InlineFerris,
        _: &mut DocumentRenderContext<'_>,
        output: &mut DocumentWriter,
    ) -> Result<(), DocumentRenderError> {
        output.write_str("<span class=\"ferris-inline\">🦀</span>")?;
        Ok(())
    }
}

// This is an extension for the inline subparser.
struct FerrisInlineScanner;

impl InlineRule for FerrisInlineScanner {
    // This is a character that starts your custom structure
    // (other characters may get skipped over).
    const MARKER: char = '(';

    // This is a custom function that will be invoked on every character
    // in an inline context.
    //
    // It should inspect `state.remaining()` and report if your custom structure
    // appears at the current position.
    //
    // If custom structure is found, it:
    //  - creates a new `NodeDraft`
    //  - returns length of it
    //
    fn run(state: &mut DocumentInlineState<'_>) -> Option<(Option<NodeDraft>, usize)> {
        if !state.remaining().starts_with(CRAB_CLAW) {
            return None;
        }

        // return new node and length of this structure
        Some((Some(NodeDraft::new(InlineFerris)), CRAB_CLAW.len()))
    }
}

pub fn add(md: &mut MarkdownIt) {
    // insert this rule into inline subparser
    md.inline.add_rule::<FerrisInlineScanner>();
    md.add_document_renderer::<InlineFerris, _>("html", InlineFerrisDocumentRenderer);
}
