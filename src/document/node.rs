use std::any::TypeId;

use super::NodeId;
use super::data::NodeData;
use crate::common::sourcemap::SourcePos;
use crate::parser::extset::NodeExtSet;
use crate::parser::node::{HtmlAttributes, NodeEmpty, NodeValue};

/// Borrowed view of a node produced by a structural traversal.
pub type NodeRef<'a> = &'a DocumentNode;

/// Data for one arena-backed document node.
#[derive(Debug)]
pub struct DocumentNode {
    pub(super) id: NodeId,
    pub(super) parent: Option<NodeId>,
    pub(super) children: Vec<NodeId>,
    pub(super) data: NodeData,
}

impl DocumentNode {
    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn srcmap(&self) -> Option<SourcePos> {
        self.data.srcmap
    }

    pub(crate) fn set_srcmap(&mut self, srcmap: Option<SourcePos>) {
        self.data.srcmap = srcmap;
    }

    pub fn ext(&self) -> &NodeExtSet {
        &self.data.ext
    }

    pub fn attrs(&self) -> &HtmlAttributes {
        &self.data.attrs
    }

    pub(crate) fn attrs_mut(&mut self) -> &mut HtmlAttributes {
        &mut self.data.attrs
    }

    pub fn name(&self) -> &'static str {
        self.data.name()
    }

    pub(crate) fn type_id(&self) -> TypeId {
        self.data.type_id()
    }

    pub fn is<T: NodeValue>(&self) -> bool {
        self.data.is::<T>()
    }

    pub fn cast<T: NodeValue>(&self) -> Option<&T> {
        self.data.cast::<T>()
    }

    pub(crate) fn cast_mut<T: NodeValue>(&mut self) -> Option<&mut T> {
        self.data.cast_mut::<T>()
    }
}

/// Owned node data waiting to be inserted into a [`Document`](super::Document).
///
/// A draft does not have a [`NodeId`] until its edit batch commits. New drafts
/// intentionally have no source mapping: generated nodes must not pretend to
/// originate from a range in the original Markdown source.
#[derive(Debug)]
pub struct NodeDraft {
    pub(super) children: Vec<NodeDraft>,
    pub(super) data: NodeData,
}

impl NodeDraft {
    /// Create a generated node with no children, attributes, extensions, or
    /// source mapping.
    pub fn new<T: NodeValue>(value: T) -> Self {
        Self {
            children: Vec::new(),
            data: NodeData::new(value),
        }
    }

    pub fn children(&self) -> &[NodeDraft] {
        &self.children
    }

    pub fn children_mut(&mut self) -> &mut Vec<NodeDraft> {
        &mut self.children
    }

    pub fn push_child(&mut self, child: NodeDraft) {
        self.children.push(child);
    }

    pub fn srcmap(&self) -> Option<SourcePos> {
        self.data.srcmap
    }

    pub fn set_srcmap(&mut self, srcmap: Option<SourcePos>) {
        self.data.srcmap = srcmap;
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

    pub(super) fn into_parts(mut self) -> (Vec<NodeDraft>, NodeData) {
        // Drop prevents moving fields out directly
        let children = std::mem::take(&mut self.children);
        let data = std::mem::replace(&mut self.data, NodeData::new(NodeEmpty));
        (children, data)
    }

    pub(crate) fn replace<T: NodeValue>(&mut self, value: T) {
        self.data.replace::<T>(value);
    }
}

impl Drop for NodeDraft {
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.children);
        // use iteration instead of recursion
        // avoid stack overflow when nested deeply
        while let Some(mut child) = pending.pop() {
            pending.append(&mut child.children);
        }
    }
}
