//! Experimental arena-backed document storage.

mod arena;
mod bridge;
mod data;
pub mod edit;
mod events;
mod node;
mod structure;
pub mod text;
pub mod transform;

use std::sync::Arc;

use self::arena::Arena;
pub use self::arena::NodeId;
pub use self::events::{StructuralEvent, StructuralEvents};
pub use self::node::{DocumentNode, NodeDraft, NodeRef};
pub(crate) use self::structure::SiblingPosition;
use crate::parser::node::Node;

/// Arena-backed representation of one parsed Markdown document.
///
/// This is currently an opt-in migration API. Existing [`crate::MarkdownIt::parse`]
/// callers continue to receive a legacy [`Node`] tree.
#[derive(Debug)]
pub struct Document {
    source: Arc<str>,
    arena: Arena<DocumentNode>,
    root: NodeId,
}

impl Document {
    pub(crate) fn from_draft(source: impl Into<Arc<str>>, root: NodeDraft) -> Self {
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
        self.arena.len
    }

    pub fn is_empty(&self) -> bool {
        self.arena.len == 0
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
    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut DocumentNode {
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
