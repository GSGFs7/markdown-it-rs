use std::fmt;
use std::sync::Arc;

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::Document;
use crate::document::edit::EditBatch;

/// Prepare shared state from the source. Runs before or after block parsing,
/// as placed in the core ruler.
pub type PrepareStateFn = fn(&str, &MarkdownIt, &mut RootExtSet);

/// Finalize the document from shared state. Runs after inline
/// parsing, before postprocessing.
pub type FinalizeFn = fn(&mut Document, &RootExtSet);

/// A document postprocessing pass: observes the finished document and
/// returns the edits to apply.
#[derive(Clone)]
pub struct PostProcess {
    run: Arc<dyn Fn(&Document) -> EditBatch + Send + Sync>,
}

impl PostProcess {
    pub fn new(run: impl Fn(&Document) -> EditBatch + Send + Sync + 'static) -> Self {
        Self { run: Arc::new(run) }
    }

    pub(crate) fn run(&self, document: &Document) -> EditBatch {
        (self.run)(document)
    }
}

impl fmt::Debug for PostProcess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PostProcess").finish_non_exhaustive()
    }
}

/// Execution stage of a core rule.
///
/// Registered through [`CoreRule::stage`]. Stages run in this order:
///
/// ```text
/// PrepareState (before blocks) -> ParseBlocks -> PrepareState (after blocks)
///     -> ParseInlines -> Finalize -> persist RootExtSet
///     -> PostProcess -> Document
/// ```
///
/// `ParseBlocks` and `ParseInlines` are required once each, in order; other
/// callbacks must respect their stages. Postprocessors may be placed anywhere
/// but always run after finalizers, in ruler order. The pure-text fast path
/// also runs preparations, finalizers, and postprocessors.
#[derive(Debug, Clone)]
pub enum CoreStage {
    /// Build block nodes directly in the arena, deferring inline content until all block
    /// definitions are available. Marks the built-in block parser.
    ParseBlocks,
    /// Resolve deferred inline content from block-pass state. Must follow
    /// `ParseBlocks`; marks the built-in inline parser.
    ParseInlines,
    /// Prepare [`RootExtSet`] from the source. Must precede `ParseInlines`;
    /// placed before `ParseBlocks` it runs before block parsing, otherwise
    /// after (with block state available).
    PrepareState(PrepareStateFn),
    /// Modify the resolved [`Document`], e.g. to move footnote definitions to
    /// the end. Must follow `ParseInlines`; state is read-only and persisted to
    /// `Root.ext` after all finalizers.
    Finalize(FinalizeFn),
    /// Inspect the finished document and commit a batch of edits. May be placed
    /// anywhere in the ruler, but always runs after finalizers and `Root.ext`
    /// persistence, in ruler order. Can be rerun with
    /// [`MarkdownIt::run_postprocessors`].
    PostProcess(PostProcess),
}

/// Trait implemented by all core rules.
pub trait CoreRule: 'static {
    const NAMES: &'static [&'static str] = &[];

    /// Select the execution stage and callback for this rule.
    fn stage(&self) -> CoreStage;
}

crate::parser::rule::rule_builder!(CoreRule);
