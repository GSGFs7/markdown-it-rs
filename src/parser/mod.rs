//! Parsing pipeline and its extension points.
//!
//! As in markdown-it, parsing is split into three rule chains:
//!
//! - [`core`] defines whole-document rules and the shared parsing state; the
//!   chain itself is owned and run by [`crate::MarkdownIt`].
//! - [`block`] builds the block structure. [`block::BlockParser`] and
//!   [`block::BlockState`] own block rule registration and parsing state.
//! - [`inline`] tokenizes inline content. The built-in inline pass runs after
//!   block parsing, so document-wide definitions such as link references are
//!   already available.
//!
//! Add new syntax with [`block::BlockParser::add_rule`],
//! [`inline::InlineParser::add_rule`], or [`crate::MarkdownIt::add_rule`]. Each
//! returns a builder for ordering the new rule relative to existing ones.
pub mod block;
pub mod core;
pub mod inline;
pub(crate) mod pipeline;
mod rule;
