//! Document parsing through transient drafts and arena storage.

use std::sync::Arc;

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeDraft, NodeValue, Root, Text};
use crate::parser::block::{DocumentBlockState, build_line_offsets};
use crate::parser::core::{DocumentCoreRule, DocumentFinalizeDraftFn, DocumentPrepareStateFn};
use crate::parser::inline::{DocumentInlineState, DocumentRuleSet};

/// Inline content queued during the block pass and resolved once all
/// reference definitions have been collected.
#[derive(Debug)]
pub(super) struct PendingInline {
    pub(super) content: String,
    pub(super) mapping: Vec<(usize, usize)>,
}

impl NodeValue for PendingInline {}

/// Replace every [`PendingInline`] draft in `draft` with parsed inline nodes.
fn resolve_pending_inline(
    draft: &mut NodeDraft,
    md: &MarkdownIt,
    ruleset: &DocumentRuleSet,
    root_ext: &RootExtSet,
) {
    let children = std::mem::take(draft.children_mut());
    draft.children_mut().reserve(children.len());
    for mut child in children {
        if let Some(pending) = child.cast_mut::<PendingInline>() {
            let content = std::mem::take(&mut pending.content);
            let mapping = std::mem::take(&mut pending.mapping);
            draft.children_mut().extend(DocumentInlineState::parse(
                content,
                mapping,
                md,
                ruleset,
                Some(root_ext),
            ));
        } else {
            stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                resolve_pending_inline(&mut child, md, ruleset, root_ext);
            });
            draft.push_child(child);
        }
    }
}

pub(crate) struct DocumentParseContext<'a> {
    source: Arc<str>,
    md: &'a MarkdownIt,
    root: NodeDraft,
    root_ext: RootExtSet,
    inline_preparations: Vec<DocumentPrepareStateFn>,
    draft_finalizers: Vec<DocumentFinalizeDraftFn>,
}

impl<'a> DocumentParseContext<'a> {
    pub(crate) fn new(source: Arc<str>, md: &'a MarkdownIt, mut root: NodeDraft) -> Self {
        root.set_srcmap(Some(SourcePos::new(0, source.len())));
        Self {
            source,
            md,
            root,
            root_ext: RootExtSet::new(),
            inline_preparations: Vec::new(),
            draft_finalizers: Vec::new(),
        }
    }

    pub(crate) fn parse(mut self) -> Document {
        let mut preparations = Vec::new();
        let mut seen_block = false;
        let mut seen_inline = false;
        let mut supported = true;
        for rule in self.md.document_core_rules() {
            match rule {
                DocumentCoreRule::ParseBlocks => {
                    supported &= !seen_block && !seen_inline;
                    seen_block = true;
                }
                DocumentCoreRule::ParseInlines => {
                    supported &= seen_block && !seen_inline;
                    seen_inline = true;
                }
                DocumentCoreRule::PrepareState(prepare) => {
                    // Preserve whether source analysis runs before or after blocks.
                    supported &= !seen_inline;
                    if seen_block {
                        self.inline_preparations.push(prepare);
                    } else {
                        preparations.push(prepare);
                    }
                }
                DocumentCoreRule::FinalizeDraft(finalize) => {
                    // Finalizers must follow the inline pass.
                    supported &= seen_inline;
                    self.draft_finalizers.push(finalize);
                }
            }
        }
        assert!(
            supported && seen_block && seen_inline,
            "direct parsing requires exactly one block stage followed by exactly one inline stage, supported core rules, preparations before inlines, and draft finalizers after inlines",
        );

        let block_rules = self.md.block.document_rules();
        let inline_rules = self.md.inline.document_rules();

        for prepare in preparations {
            prepare(&self.source, self.md, &mut self.root_ext);
        }
        if block_rules.is_empty() && self.md.inline.has_only_text_rule() && self.md.max_nesting > 0
        {
            self.prepare_inlines();
            return self.parse_text_fallback();
        }

        let mut state = DocumentBlockState::new(&self.source, self.md, block_rules, self.root);
        state.root_ext = self.root_ext;
        state.tokenize();
        self.root = state.node;
        self.root_ext = state.root_ext;
        self.prepare_inlines();

        // Inline parsing runs after the block pass so later reference
        // definitions can resolve earlier uses.
        resolve_pending_inline(&mut self.root, self.md, &inline_rules, &self.root_ext);
        self.finish()
    }

    fn prepare_inlines(&mut self) {
        for prepare in &self.inline_preparations {
            prepare(&self.source, self.md, &mut self.root_ext);
        }
    }

    fn finish(mut self) -> Document {
        // Post-inline core rules may now reorder the resolved draft.
        for finalize in self.draft_finalizers {
            finalize(&mut self.root, &self.root_ext);
        }
        // Persist the cross-block extension set on the root payload.
        if let Some(data) = self.root.cast_mut::<Root>() {
            data.ext = self.root_ext;
        }

        let mut document = Document::from_draft(self.source, self.root);
        self.md.run_document_transforms(&mut document);
        document
    }

    fn parse_text_fallback(mut self) -> Document {
        for line in build_line_offsets(&self.source) {
            if line.first_nonspace >= line.line_end {
                continue;
            }

            let mut content = self.source[line.first_nonspace..line.line_end].to_owned();
            content.push('\n');
            let mut text = NodeDraft::new(Text { content });
            text.set_srcmap(Some(SourcePos::new(line.first_nonspace, line.line_end + 1)));
            self.root.push_child(text);
        }

        self.finish()
    }
}
