//! Markdown parsing directly into arena-backed documents.

use std::sync::Arc;

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::common::sourcemap::SourcePos;
use crate::document::{Document, NodeId, NodeValue, Root, Text};
use crate::parser::block::{DocumentBlockState, build_line_offsets};
use crate::parser::core::{DocumentCoreRule, DocumentFinalizeDocumentFn, DocumentPrepareStateFn};
use crate::parser::inline::{DocumentInlineState, DocumentRuleSet};

/// Inline content queued during the block pass, resolved once all reference
/// definitions have been collected.
#[derive(Debug)]
pub(super) struct PendingInline {
    pub(super) content: String,
    pub(super) mapping: Vec<(usize, usize)>,
}

impl NodeValue for PendingInline {}

/// Resolve deferred content in place after collecting all block definitions.
///
/// Pending nodes must be resolved in source order: inline footnotes allocate
/// their numbers during this walk.
///
/// associated pathological test: `deferred_inline_siblings`
fn resolve_pending_inline(
    document: &mut Document,
    md: &MarkdownIt,
    ruleset: &DocumentRuleSet,
    root_ext: &RootExtSet,
) {
    /// A parent on the traversal stack and the replacements collected for it.
    struct Frame {
        parent: NodeId,
        next_child: usize,
        replacements: Vec<(usize, Vec<NodeId>)>,
    }

    impl Frame {
        fn new(parent: NodeId) -> Self {
            Self {
                parent,
                next_child: 0,
                replacements: Vec::new(),
            }
        }

        /// Replace the placeholders buffered during the walk with their
        /// resolved nodes.
        fn apply(self, document: &mut Document) {
            if self.replacements.is_empty() {
                return;
            }

            document.rewrite_children(self.parent, |document, children| {
                let mut replacements = self.replacements.into_iter().peekable();
                let old_children = std::mem::take(children);
                children.reserve(old_children.len());

                for (index, child) in old_children.into_iter().enumerate() {
                    if let Some((_, nodes)) =
                        replacements.next_if(|(position, _)| *position == index)
                    {
                        document.discard_node(child);
                        children.extend(nodes);
                    } else {
                        children.push(child);
                    }
                }
            });
        }
    }

    let mut stack = vec![Frame::new(document.root())];
    while let Some(frame) = stack.last_mut() {
        let index = frame.next_child;
        let Some(&child) = document.children(frame.parent).get(index) else {
            let frame = stack.pop().unwrap();
            frame.apply(document);
            continue;
        };
        frame.next_child = index + 1;

        if let Some(inline) = document.node_mut(child).cast_mut::<PendingInline>() {
            let content = std::mem::take(&mut inline.content);
            let mapping = std::mem::take(&mut inline.mapping);
            let nodes =
                DocumentInlineState::parse(document, content, mapping, md, ruleset, Some(root_ext));
            frame.replacements.push((index, nodes));
        } else if !document.children(child).is_empty() {
            stack.push(Frame::new(child));
        }
    }
}

pub(crate) struct DocumentParseContext<'a> {
    source: Arc<str>,
    md: &'a MarkdownIt,
    document: Document,
    root_ext: RootExtSet,
    inline_preparations: Vec<DocumentPrepareStateFn>,
    document_finalizers: Vec<DocumentFinalizeDocumentFn>,
}

impl<'a> DocumentParseContext<'a> {
    pub(crate) fn new(source: Arc<str>, md: &'a MarkdownIt, mut document: Document) -> Self {
        document
            .node_mut(document.root())
            .set_srcmap(Some(SourcePos::new(0, source.len())));
        Self {
            source,
            md,
            document,
            root_ext: RootExtSet::new(),
            inline_preparations: Vec::new(),
            document_finalizers: Vec::new(),
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
                DocumentCoreRule::FinalizeDocument(finalize) => {
                    // Finalizers must follow the inline pass.
                    supported &= seen_inline;
                    self.document_finalizers.push(finalize);
                }
            }
        }

        assert!(
            supported && seen_block && seen_inline,
            "direct parsing requires one block stage, then one inline stage, with correctly ordered core rules",
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

        let mut state = DocumentBlockState::new(&self.source, self.md, block_rules, self.document);
        state.root_ext = self.root_ext;
        state.tokenize();
        self.document = state.document;
        self.root_ext = state.root_ext;
        self.prepare_inlines();

        resolve_pending_inline(&mut self.document, self.md, &inline_rules, &self.root_ext);
        self.finish()
    }

    fn prepare_inlines(&mut self) {
        for prepare in &self.inline_preparations {
            prepare(&self.source, self.md, &mut self.root_ext);
        }
    }

    fn finish(mut self) -> Document {
        // Post-inline core rules may now reorder the resolved document.
        for finalize in self.document_finalizers {
            finalize(&mut self.document, &self.root_ext);
        }

        // Persist the cross-block extension set on the root payload.
        if let Some(data) = self
            .document
            .node_mut(self.document.root())
            .cast_mut::<Root>()
        {
            data.ext = self.root_ext;
        }

        let mut document = self.document;
        document.trim_unused_tail();
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
            let text = self.document.create_node(Text { content });
            self.document
                .node_mut(text)
                .set_srcmap(Some(SourcePos::new(line.first_nonspace, line.line_end + 1)));
            self.document.push_child(self.document.root(), text);
        }

        self.finish()
    }
}
