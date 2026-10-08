//! Parser and its extension points.
//!
//! Parsing runs three ordered rule chains:
//!  - [`inline`] — on each inline character
//!  - [`block`] — on each line
//!  - [`core`] — once per document
//!
//! Extend them via [`inline::InlineParser::add_rule`],
//! [`block::BlockParser::add_rule`], and [`crate::MarkdownIt::add_rule`].
pub mod block;
pub mod core;
pub mod document_parser;
pub mod extset;
pub mod inline;
pub mod linkfmt;

pub(super) mod main;
pub(super) mod node;
pub(super) mod render_options;
