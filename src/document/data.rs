use std::any::TypeId;

use crate::common::TypeKey;
use crate::common::sourcemap::SourcePos;
use crate::parser::extset::NodeExtSet;
use crate::parser::node::{HtmlAttributes, NodeValue};

#[derive(Debug)]
pub(super) struct NodeData {
    pub(super) srcmap: Option<SourcePos>,
    pub(super) ext: NodeExtSet,
    pub(super) attrs: HtmlAttributes,
    node_type: TypeKey,
    node_value: Box<dyn NodeValue>,
}

impl NodeData {
    pub(super) fn new<T: NodeValue>(value: T) -> Self {
        Self {
            srcmap: None,
            ext: NodeExtSet::new(),
            attrs: Vec::new(),
            node_type: TypeKey::of::<T>(),
            node_value: Box::new(value),
        }
    }

    pub(super) fn name(&self) -> &'static str {
        self.node_type.name
    }

    pub(super) fn type_id(&self) -> TypeId {
        self.node_type.id
    }

    pub(super) fn is<T: NodeValue>(&self) -> bool {
        self.type_id() == TypeId::of::<T>()
    }

    pub(super) fn cast<T: NodeValue>(&self) -> Option<&T> {
        if self.is::<T>() {
            self.node_value.downcast_ref::<T>()
        } else {
            None
        }
    }

    pub(super) fn cast_mut<T: NodeValue>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            self.node_value.downcast_mut::<T>()
        } else {
            None
        }
    }

    pub(super) fn replace<T: NodeValue>(&mut self, value: T) {
        self.node_type = TypeKey::of::<T>();
        self.node_value = Box::new(value);
    }

    pub(super) fn replace_value(&mut self, replacement: Self) {
        self.node_type = replacement.node_type;
        self.node_value = replacement.node_value;
    }
}
