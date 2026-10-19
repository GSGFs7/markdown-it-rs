//! Parser and its extension points.
//!
//! [`block`]/[`inline`] own rule registration and parsing state, and [`core`]
//! defines ordered document processing rules. Extend parsing via
//! [`block::BlockParser::add_rule`], [`inline::InlineParser::add_rule`], and
//! [`crate::MarkdownIt::add_rule`].
pub mod block;
pub mod core;
pub mod inline;

pub(crate) mod pipeline;
mod rule;

#[cfg(debug_assertions)]
mod validation;

/// Mapping from inline byte offsets to document byte offsets.
///
/// Without the `sourcemap` feature this is a zero-sized carrier; custom block
/// rules can use `InlineMapping::default()` when source mapping is disabled.
#[cfg(feature = "sourcemap")]
pub type InlineMapping = Vec<(usize, usize)>;

/// Zero-sized mapping carrier used when source mapping is disabled.
#[cfg(not(feature = "sourcemap"))]
#[derive(Debug, Default, Clone, Copy)]
pub struct InlineMapping;
