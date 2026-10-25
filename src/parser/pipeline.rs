//! Deferred inline parsing after the block pass.

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{Document, NodeId};
use crate::parser::inline::{InlineState, RuleSet};

/// Inline content queued during the block pass, resolved once all reference
/// definitions have been collected.
#[derive(Debug)]
pub(super) struct PendingInline {
    pub(super) content: String,
    pub(super) mapping: crate::parser::InlineMapping,
}

/// Resolve deferred content in place after collecting all block definitions.
///
/// Pending nodes must be resolved in source order: inline footnotes allocate
/// their numbers during this walk.
///
/// associated pathological test: `deferred_inline_siblings`
pub(super) fn resolve_pending_inline(
    document: &mut Document,
    md: &MarkdownIt,
    ruleset: &RuleSet,
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
            let nodes = InlineState::parse(document, content, mapping, md, ruleset, Some(root_ext));
            if index == 0 && document.children(frame.parent).len() == 1 {
                // if only one child, replace the children.
                let parent = frame.parent;
                document.rewrite_children(parent, |document, children| {
                    document.discard_node(children.pop().unwrap());
                    *children = nodes;
                });
                stack.pop();
            } else {
                frame.replacements.push((index, nodes));
            }
        } else if !document.children(child).is_empty() {
            stack.push(Frame::new(child));
        }
    }
}
