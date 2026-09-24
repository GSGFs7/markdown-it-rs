use std::iter::FusedIterator;

use super::node::NodeRef;
use super::{Document, NodeId};

/// One step in a depth-first structural traversal.
///
/// Nodes with children produce a balanced `Enter` / `Exit` pair. Nodes
/// without children produce exactly one `Leaf` event.
#[derive(Clone, Copy, Debug)]
pub enum StructuralEvent<'a> {
    Enter(NodeRef<'a>),
    Leaf(NodeRef<'a>),
    Exit(NodeRef<'a>),
}

impl<'a> StructuralEvent<'a> {
    pub fn node(&self) -> NodeRef<'a> {
        match self {
            Self::Enter(node) | Self::Leaf(node) | Self::Exit(node) => node,
        }
    }
}

#[derive(Clone, Debug)]
struct EventFrame<'a> {
    node: NodeRef<'a>,
    children: std::slice::Iter<'a, NodeId>,
}

/// Lazy depth-first iterator over a document subtree.
///
/// The iterator allocates only a stack proportional to the current nesting
/// depth. The document's parent/children links remain the sole structural
/// source of truth.
#[derive(Debug)]
pub struct StructuralEvents<'a> {
    document: &'a Document,
    stack: Vec<EventFrame<'a>>,
    pending: Option<StructuralEvent<'a>>,
}

impl<'a> Iterator for StructuralEvents<'a> {
    type Item = StructuralEvent<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(event) = self.pending.take() {
            return Some(event);
        }

        let frame = self.stack.last_mut()?;
        let node = frame.node;

        if let Some(&child) = frame.children.next() {
            let child = self
                .document
                .arena
                .get(child)
                .expect("document tree is internally valid");
            if child.children.is_empty() {
                return Some(StructuralEvent::Leaf(child));
            }
            self.stack.push(EventFrame {
                node: child,
                children: child.children.iter(),
            });
            return Some(StructuralEvent::Enter(child));
        }

        self.stack.pop();
        Some(StructuralEvent::Exit(node))
    }
}

impl FusedIterator for StructuralEvents<'_> {}

impl Document {
    /// Lazily traverse a node and its descendants in depth-first order.
    ///
    /// # Panics
    /// Panics if the root ID is invalid or stale.
    #[track_caller]
    pub fn events(&self, root: NodeId) -> StructuralEvents<'_> {
        let root = self.node(root);
        let mut stack = Vec::with_capacity(16);
        let pending = if root.children.is_empty() {
            Some(StructuralEvent::Leaf(root))
        } else {
            stack.push(EventFrame {
                node: root,
                children: root.children.iter(),
            });
            Some(StructuralEvent::Enter(root))
        };
        StructuralEvents {
            document: self,
            stack,
            pending,
        }
    }
}
