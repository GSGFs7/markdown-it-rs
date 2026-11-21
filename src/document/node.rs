use std::any::TypeId;

use super::NodeId;
use super::data::NodeData;
use crate::common::extset::NodeExtSet;
use crate::document::{HtmlAttributes, NodeValue};

/// Data for one document node.
#[derive(Debug)]
pub struct Node {
    pub(super) id: NodeId,
    pub(super) parent: Option<NodeId>,
    pub(super) children: Vec<NodeId>,
    pub(super) data: NodeData,
}

impl Node {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Best-effort source offsets for this node.
    ///
    /// This is an unvalidated hint that may refer to a stale or invalid range.
    /// Use `document.source().get(start..end)` to check the bounds and UTF-8
    /// boundaries before reading the source text.
    #[cfg(feature = "sourcemap")]
    pub fn srcmap(&self) -> Option<crate::common::sourcemap::SourcePos> {
        self.data.srcmap
    }

    /// Set or clear the unvalidated source hint. Prefer
    /// [`EditBatch::set_source_map`](crate::document::edit::EditBatch::set_source_map) in edit batches.
    #[cfg(feature = "sourcemap")]
    pub fn set_srcmap(&mut self, srcmap: Option<crate::common::sourcemap::SourcePos>) {
        self.data.srcmap = srcmap;
    }

    /// Whether this node is an internal transparent wrapper.
    ///
    /// Transparent nodes render their children in every format, including when
    /// they have no children.
    pub(crate) fn is_transparent(&self) -> bool {
        self.data.transparent
    }

    pub(crate) fn set_transparent(&mut self, transparent: bool) {
        self.data.transparent = transparent;
    }

    pub fn ext(&self) -> &NodeExtSet {
        &self.data.ext
    }

    pub fn ext_mut(&mut self) -> &mut NodeExtSet {
        &mut self.data.ext
    }

    pub fn attrs(&self) -> &HtmlAttributes {
        &self.data.attrs
    }

    pub fn attrs_mut(&mut self) -> &mut HtmlAttributes {
        &mut self.data.attrs
    }

    pub fn name(&self) -> &'static str {
        self.data.name()
    }

    pub fn is<T: NodeValue>(&self) -> bool {
        self.data.is::<T>()
    }

    pub fn cast<T: NodeValue>(&self) -> Option<&T> {
        self.data.cast::<T>()
    }

    pub fn cast_mut<T: NodeValue>(&mut self) -> Option<&mut T> {
        self.data.cast_mut::<T>()
    }

    pub(crate) fn type_id(&self) -> TypeId {
        self.data.type_id()
    }

    pub(crate) fn replace_value<T: NodeValue>(&mut self, value: T) {
        self.data.replace_value(NodeData::new(value));
    }
}
