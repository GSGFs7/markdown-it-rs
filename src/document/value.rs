use std::fmt::Debug;

use downcast_rs::{Downcast, impl_downcast};

/// One HTML attribute: `(name, value)`.
pub type HtmlAttribute = (String, String);
pub type HtmlAttributes = Vec<HtmlAttribute>;

/// Typed payload stored in a document node. Rendering is registered separately.
///
/// Automatically implemented for every `Debug + Send + Sync + 'static` type.
/// Custom payloads do not need an explicit implementation.
pub trait NodeValue: Debug + Downcast + Send + Sync {}
impl<T: Debug + Downcast + Send + Sync> NodeValue for T {}
impl_downcast!(NodeValue);

#[derive(Debug)]
/// Temporary payload used when moving data out of a draft during destruction.
pub(crate) struct NodeEmpty;
