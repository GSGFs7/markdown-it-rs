//! Plugin authoring examples for third-party plugins.
//!
//! Each module is a small, self-contained plugin. The suite is also compiled as
//! an independent crate (see `Cargo.toml`), so every example must work using
//! only the public API.
//!
//! | Module | Demonstrates |
//! | --- | --- |
//! | [`admonition`] | A block container that parses a nested block body. |
//! | [`container`] | An inline container that parses its body in a child session. |
//! | [`counted`] | Shared counter state and ordered finish callbacks. |
//! | [`gh_alert`] | Rewriting blockquotes into GitHub-style alerts. |
//! | [`label`] | A configured inline leaf node. |
//! | [`notice`] | A configured block leaf node. |
//! | [`wrap`] | Wrapping inline nodes in the AST after inline parsing. |

pub mod admonition;
pub mod container;
pub mod counted;
pub mod gh_alert;
pub mod label;
pub mod notice;
pub mod wrap;
