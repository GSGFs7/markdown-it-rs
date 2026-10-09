use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::Document;

/// Prepare shared state from the source. Runs before or after block parsing,
/// as placed in the core ruler.
pub type DocumentPrepareStateFn = fn(&str, &MarkdownIt, &mut RootExtSet);

/// Finalize the arena-backed document from shared state. Runs after inline
/// parsing, before document transforms.
pub type DocumentFinalizeDocumentFn = fn(&mut Document, &RootExtSet);

/// Execution stage of a document core rule.
///
/// Registered through [`CoreRule::document_rule`]. Stages run in this order:
///
/// ```text
/// PrepareState (before blocks) -> ParseBlocks -> PrepareState (after blocks)
///     -> ParseInlines -> FinalizeDocument -> persist RootExtSet -> Document
/// ```
///
/// Exactly one `ParseBlocks` and one `ParseInlines` are required, in that order;
/// callbacks outside their stage are rejected and preserve core-ruler order.
/// The pure-text fast path still runs preparations and finalizers. Arena-backed
/// document transforms run after finalizers.
#[derive(Debug, Clone, Copy)]
pub enum DocumentCoreRule {
    /// Build block nodes directly in the arena, deferring inline content until all block
    /// definitions are available. Marks the built-in block parser.
    ParseBlocks,
    /// Resolve deferred inline content from block-pass state. Must follow
    /// `ParseBlocks`; marks the built-in inline parser.
    ParseInlines,
    /// Prepare [`RootExtSet`] from the source. Must precede `ParseInlines`;
    /// placed before `ParseBlocks` it runs before block parsing, otherwise
    /// after (with block state available).
    PrepareState(DocumentPrepareStateFn),
    /// Modify the resolved [`Document`], e.g. to move footnote definitions to
    /// the end. Must follow `ParseInlines`; state is read-only and persisted to
    /// `Root.ext` after all finalizers.
    FinalizeDocument(DocumentFinalizeDocumentFn),
}

/// Each member of core rule chain must implement this trait
pub trait CoreRule: 'static {
    const NAMES: &'static [&'static str] = &[];

    /// Select the execution stage and callback for this rule.
    fn document_rule() -> DocumentCoreRule;
}

crate::parser::rule::rule_builder!(CoreRule);
