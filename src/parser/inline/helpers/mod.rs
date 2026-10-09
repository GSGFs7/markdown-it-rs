//! Reusable rules for building custom inline syntax.
//!
//! These helpers implement configurable code spans, emphasis pairs, and links.
//! Register them with a node factory, then provide a renderer for your node type.

pub mod code_pair;
pub mod delimiters;
pub mod emph_pair;
pub mod full_link;
