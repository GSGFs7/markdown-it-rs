//! Parser and its extension points.
//!
//! [`block`]/[`inline`] own rule registration and parsing state, and [`core`]
//! defines document-stage rules. Extend parsing via
//! [`block::BlockParser::add_rule`], [`inline::InlineParser::add_rule`], and
//! [`crate::MarkdownIt::add_rule`].
pub mod block;
pub mod core;
pub mod inline;

pub(crate) mod pipeline;
mod rule;
