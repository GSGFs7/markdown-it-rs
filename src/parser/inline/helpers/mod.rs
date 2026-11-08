//! Reusable building blocks for custom inline syntax.
//!
//! | Helper | Use when | Behavior |
//! | ------ | -------- | -------- |
//! | [`code_pair`] | You need CommonMark code-span-like syntax | Marker runs must match in length; the body is literal |
//! | [`emph_pair`] | You need emphasis-like delimiters with fixed length | The same marker opens and closes; the body is parsed as inline children |
//! | [`full_link`] | You need link- or image-like `label(target "title")` syntax | Handles label, destination, title, and reference fallback |
//! | [`paired_span`] | You have arbitrary open/close markers such as `{{...}}` | Scans boundaries only; the caller decides whether the body is literal or parsed |
//! | [`delimiters`] | You are implementing custom emphasis-like classification | Lower-level delimiter scanning, not a complete pair registration API |
//!
//! [`code_pair`], [`emph_pair`], and [`full_link`] register complete inline rules.
//! [`paired_span`] is only a boundary scanner and is usually called from a
//! [`PluginSpec`] parse callback when none of the specialized helpers fit.
//!
//! [`PluginSpec`]: crate::plugin::PluginSpec

pub mod code_pair;
pub mod delimiters;
pub mod emph_pair;
pub mod full_link;
pub mod paired_span;
