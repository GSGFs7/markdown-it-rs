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
pub use self::arena::{InvalidNodeId, NodeId};
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

    /// Look up a node, rejecting IDs with an unknown slot or stale generation.
    pub fn node(&self, id: NodeId) -> Result<&DocumentNode, InvalidNodeId> {
        self.arena.get(id).ok_or(InvalidNodeId(id))
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> Result<&mut DocumentNode, InvalidNodeId> {
        self.arena.get_mut(id).ok_or(InvalidNodeId(id))
    }

    /// Look up a node's parent.
    pub fn parent(&self, id: NodeId) -> Result<Option<NodeId>, InvalidNodeId> {
        Ok(self.node(id)?.parent())
    }

    /// Look up a node's ordered children.
    pub fn children(&self, id: NodeId) -> Result<&[NodeId], InvalidNodeId> {
        Ok(self.node(id)?.children())
    }
}

#[cfg(test)]
mod tests;
