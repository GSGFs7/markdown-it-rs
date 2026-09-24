use std::sync::Arc;

use super::arena::Arena;
use super::data::NodeData;
use super::node::DocumentNode;
use super::{Document, Node, NodeId};

impl Document {
    /// Move an existing AST into arena-backed storage.
    pub fn from_legacy(source: impl Into<Arc<str>>, root: Node) -> Self {
        let mut document = Self {
            source: source.into(),
            arena: Arena::new(),
            // Replaced before this constructor returns.
            root: NodeId {
                slot: u32::MAX,
                generation: u32::MAX,
            },
        };
        document.root = document.insert_legacy(None, root);
        document
    }

    fn insert_legacy(&mut self, parent: Option<NodeId>, mut legacy: Node) -> NodeId {
        let (data, children) = NodeData::from_legacy_parts(legacy.take_parts());
        let id = self.arena.insert_with(|id| DocumentNode {
            id,
            parent,
            children: Vec::with_capacity(children.len()),
            data,
        });

        for child in children {
            let child_id = stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                self.insert_legacy(Some(id), child)
            });
            self.arena.get_mut(id).unwrap().children.push(child_id);
        }

        id
    }

    /// Rebuild the legacy tree, consuming this document.
    pub fn into_legacy(mut self) -> Node {
        self.take_legacy(self.root)
    }

    fn take_legacy(&mut self, id: NodeId) -> Node {
        let mut data = self
            .arena
            .remove(id)
            .expect("document tree is internally valid");
        let child_ids = std::mem::take(&mut data.children);
        let mut children = Vec::with_capacity(child_ids.len());
        for child_id in child_ids {
            children.push(stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
                self.take_legacy(child_id)
            }));
        }

        data.data.into_legacy(children)
    }
}
