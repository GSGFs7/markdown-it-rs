//! Self-contained modules used for miscellaneous purposes.
//!
//! These are all candidates for being separated into different crates,
//! tell me if functionality they provide is useful enough to do that.

pub mod ruler;
pub mod sourcemap;
pub mod typekey;
pub mod utils;

pub use typekey::{RuleMark, TypeKey};
