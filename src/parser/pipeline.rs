//! Deferred inline parsing after the block pass.

use crate::MarkdownIt;
use crate::common::extset::RootExtSet;
use crate::document::{Document, NodeId};
use crate::parser::inline::{InlineState, RuleSet};

/// Placeholder for inline content parsed after the block pass.
///
/// Some inline rule needs global context. For example:
///
/// ```markdown
/// [a link][ref]
///
/// [ref]: /url
/// ```
#[derive(Debug)]
pub(super) struct PendingInline {
    pub(super) content: String,
    pub(super) mapping: crate::parser::inline::InlineMapping,
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
    /// stack frame
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

        /// replace PendingInline with real Inline node
        fn apply(self, document: &mut Document) {
            if self.replacements.is_empty() {
                return;
            }

            // old:         [A, PendingInline, B]
            // replacement: [(1, [X, Y])]
            // new:         [A, X, Y, B]
            document.rewrite_children(self.parent, |document, children| {
                let mut replacements = self.replacements.into_iter().peekable();
                let old_children = std::mem::take(children);
                children.reserve(old_children.len());

                for (index, child) in old_children.into_iter().enumerate() {
                    // if replacement postion match a children, rewrite it
                    match replacements.next_if(|(position, _)| *position == index) {
                        Some((_, nodes)) => {
                            document.discard_node(child);
                            children.extend(nodes);
                        }
                        None => children.push(child),
                    }
                }
            });
        }
    }

    // DFS + explicit stack
    // find PendingInline & parse it
    let mut stack = vec![Frame::new(document.root())];
    while let Some(frame) = stack.last_mut() {
        let index = frame.next_child;
        let Some(&child) = document.children(frame.parent).get(index) else {
            stack.pop().unwrap().apply(document);
            continue;
        };
        frame.next_child += 1;

        if let Some(inline) = document.value_mut::<PendingInline>(child) {
            let content = std::mem::take(&mut inline.content);
            let mapping = std::mem::take(&mut inline.mapping);
            let nodes = InlineState::parse(document, content, mapping, md, ruleset, Some(root_ext));
            frame.replacements.push((index, nodes));
        } else if !document.children(child).is_empty() {
            // a normal container, enter it.
            stack.push(Frame::new(child));
        }
    }
}
