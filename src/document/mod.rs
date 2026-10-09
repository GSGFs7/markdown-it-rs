//! Arena-backed document storage.

mod arena;
mod data;
pub mod edit;
mod events;
mod node;
mod root;
mod structure;
pub mod text;
pub mod transform;
mod value;

use std::sync::Arc;

use self::arena::Arena;
pub use self::arena::NodeId;
pub use self::events::{StructuralEvent, StructuralEvents};
pub use self::node::{DocumentNode, NodeDraft, NodeRef};
pub use self::root::Root;
pub(crate) use self::structure::SiblingPosition;
pub use self::text::{Text, TextSpecial};
pub(crate) use self::value::NodeEmpty;
pub use self::value::{HtmlAttribute, HtmlAttributes, NodeValue};

/// Arena-backed representation of one parsed Markdown document.
///
#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    arena: Arena<DocumentNode>,
    root: NodeId,
}

impl Document {
    /// Create a document with its root allocated directly in the arena.
    pub(crate) fn new<T: NodeValue>(source: impl Into<Arc<str>>, value: T) -> Self {
        let mut arena = Arena::new();
        let root = arena.insert_with(|id| DocumentNode {
            id,
            parent: None,
            children: Vec::new(),
            data: data::NodeData::new(value),
        });
        Self {
            source: source.into(),
            arena,
            root,
        }
    }

    /// Allocate a detached node. Attach it with `push_child` before exposing the document.
    pub fn create_node<T: NodeValue>(&mut self, value: T) -> NodeId {
        self.arena.insert_with(|id| DocumentNode {
            id,
            parent: None,
            children: Vec::new(),
            data: data::NodeData::new(value),
        })
    }

    /// Attach a detached node to a parent.
    pub fn push_child(&mut self, parent: NodeId, child: NodeId) {
        assert_ne!(child, self.root, "cannot attach the document root");
        assert_ne!(parent, child, "cannot attach a node to itself");

        self.node(parent);
        if !self.children(child).is_empty() {
            let mut ancestor = Some(parent);
            while let Some(id) = ancestor {
                assert_ne!(id, child, "cannot create a cycle");
                ancestor = self.parent(id);
            }
        }

        assert!(self.node(child).parent.is_none(), "child must be detached");

        self.node_mut(child).parent = Some(parent);
        self.node_mut(parent).children.push(child);
    }

    /// Detach all children, preserving their order and IDs.
    pub(crate) fn take_children(&mut self, parent: NodeId) -> Vec<NodeId> {
        let children = std::mem::take(&mut self.node_mut(parent).children);
        for &child in &children {
            self.node_mut(child).parent = None;
        }
        children
    }

    /// Attach an ordered sequence of detached children to an empty `parent`.
    pub(crate) fn attach_children(&mut self, parent: NodeId, children: Vec<NodeId>) {
        assert!(
            self.children(parent).is_empty(),
            "parent must have no children"
        );

        for &child in &children {
            assert_ne!(child, self.root, "cannot attach the document root");
            assert_ne!(child, parent, "cannot attach a node to itself");
            assert!(self.node(child).parent.is_none(), "child must be detached");
            self.node_mut(child).parent = Some(parent);
        }
        self.node_mut(parent).children = children;
    }

    /// Detach `parent`'s children, run `rewrite`, then reattach the result.
    ///
    /// Children removed by `rewrite` must be discarded by the caller before
    /// `rewrite` returns; the resulting sequence is reattached in order.
    pub(crate) fn rewrite_children<R>(
        &mut self,
        parent: NodeId,
        rewrite: impl FnOnce(&mut Document, &mut Vec<NodeId>) -> R,
    ) -> R {
        let mut children = self.take_children(parent);
        let result = rewrite(self, &mut children);
        self.attach_children(parent, children);
        result
    }

    /// Build an arena-backed document from a draft tree and its source text.
    ///
    /// The source is kept for source mapping and access via [`Document::source`].
    pub fn from_draft(source: impl Into<Arc<str>>, root: NodeDraft) -> Self {
        let mut document = Self {
            source: source.into(),
            arena: Arena::new(),
            // Replaced before this constructor returns.
            root: NodeId {
                slot: u32::MAX,
                generation: u32::MAX,
            },
        };
        document.root = document.insert_draft_with_parent(None, root);
        document
    }

    /// Append a generated subtree to a valid parent, returning its new ID.
    /// Panics if `parent` is invalid or stale.
    pub fn append_child(&mut self, parent: NodeId, draft: NodeDraft) -> NodeId {
        self.node(parent);
        let child = self.insert_draft_with_parent(Some(parent), draft);
        self.node_mut(parent).children.push(child);
        child
    }

    pub(crate) fn trim_unused_tail(&mut self) {
        self.arena.trim_unused_tail();
    }

    /// Original Markdown source owned by this document.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// ID of the document root, which remains present for the document's life.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of live nodes.
    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.len() == 0
    }

    /// Return a node if its slot and generation are still valid.
    /// The ID must originate from this document.
    pub fn get_node(&self, id: NodeId) -> Option<&DocumentNode> {
        self.arena.get(id)
    }

    /// Access a node whose ID must be valid in this document.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn node(&self, id: NodeId) -> &DocumentNode {
        match self.arena.get(id) {
            Some(node) => node,
            None => panic!("invalid or stale node ID: {id:?}"),
        }
    }

    #[track_caller]
    pub fn node_mut(&mut self, id: NodeId) -> &mut DocumentNode {
        match self.arena.get_mut(id) {
            Some(node) => node,
            None => {
                panic!("invalid or stale node ID: {id:?}")
            }
        }
    }

    /// Return the parent of a valid node, or None for the document root.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent()
    }

    /// Return the ordered children of a valid node.
    ///
    /// # Panics
    /// Panics if the slot is unknown or the generation is stale.
    #[track_caller]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.node(id).children()
    }
}

#[cfg(test)]
mod tests;
