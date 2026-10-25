use std::sync::Arc;

use crate::common::extset::RootExtSet;

#[derive(Debug)]
/// Root node of the AST.
pub struct Root {
    pub content: Arc<str>,
    pub ext: RootExtSet,
}

impl Root {
    pub fn new(content: impl Into<Arc<str>>) -> Self {
        Self {
            content: content.into(),
            ext: RootExtSet::new(),
        }
    }
}
