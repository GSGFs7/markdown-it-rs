use std::sync::Arc;

use crate::common::extset::RootExtSet;

/// Root node of the AST.
#[derive(Debug)]
pub struct Root {
    content: Arc<str>,
    pub ext: RootExtSet,
}

impl Root {
    pub fn new(content: impl Into<Arc<str>>) -> Self {
        Self {
            content: content.into(),
            ext: RootExtSet::new(),
        }
    }

    /// Borrow the root content.
    ///
    /// Separate from [`Document::source`](crate::document::Document::source), which is the
    /// authoritative source text; differing content is allowed but discouraged.
    pub fn content(&self) -> &str {
        &self.content
    }
}
