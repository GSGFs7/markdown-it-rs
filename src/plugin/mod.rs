//! High-level plugin builders.
//!
//! There are two public plugin concepts:
//!
//! - [`InlinePlugin`]: a static inline leaf that parses and renders itself.
//! - [`PluginSpec`]: groups leaves, adds dependency ordering and installs them.
//!
//! Configured syntax is added to a [`PluginSpec`] with
//! [`PluginSpec::inline_leaf`], [`PluginSpec::block_leaf`] or
//! [`PluginSpec::inline_pair`]. All three builders accept low-level
//! [`NodeRenderer`](crate::NodeRenderer) implementations with
//! `.render_with(format, renderer)`.
//!
//! A payload type has one renderer per output format per parser. Register a
//! second plugin for the same payload type only when it uses the same rendering;
//! use a newtype when the rendering differs. Repeated registrations for a type
//! and format use the last renderer, matching [`MarkdownIt::add_renderer`].
//!
//! Static leaf:
//!
//! ```
//! use markdown_it::{Html, InlinePlugin, InlinePluginState, MarkdownIt};
//!
//! #[derive(Debug)]
//! struct Emoji;
//!
//! impl InlinePlugin for Emoji {
//!     const NAME: &'static str = "rocket";
//!     const MARKER: char = ':';
//!
//!     fn parse(cx: &mut InlinePluginState<'_>) -> Option<Self> {
//!         cx.consume(":rocket:").then_some(Emoji)
//!     }
//!
//!     fn render_html(&self, html: &mut Html) {
//!         html.element("span").text("🚀");
//!     }
//! }
//!
//! let md = MarkdownIt::builder().plugin(Emoji).build();
//! assert_eq!(md.render(":rocket:"), "<p><span>🚀</span></p>\n");
//! ```
//!
//! Configured leaf:
//!
//! ```
//! use markdown_it::{Html, MarkdownIt, PluginSpec};
//!
//! #[derive(Debug)]
//! struct Badge(&'static str);
//!
//! let badge = PluginSpec::new("badge")
//!     .inline_leaf::<Badge>("badge")
//!     .marker(':')
//!     .parse(|cx| {
//!         if !cx.consume(":badge:") {
//!             return None;
//!         }
//!         Some(Badge("new"))
//!     })
//!     .render_html(|badge, html| {
//!         html.element("span").class("badge").text(badge.0);
//!     })
//!     .finish();
//!
//! let md = MarkdownIt::builder().plugin(badge).build();
//! assert_eq!(
//!     md.render(":badge:"),
//!     "<p><span class=\"badge\">new</span></p>\n"
//! );
//! ```

mod block;
mod builder;
mod html;
mod inline;
mod pair;
mod renderer;
mod spec;
mod state;

pub use block::{BlockLeafBuilder, BlockPluginState};
pub use builder::MarkdownItBuilder;
pub use html::{Html, HtmlElement};
pub use inline::{InlineLeafBuilder, InlinePlugin, LeafRenderer, LeafRule};
pub use pair::{InlinePairBuilder, PairDelimiter};
pub use renderer::WithRenderer;
pub use spec::PluginSpec;
pub use state::InlinePluginState;

use crate::MarkdownIt;

/// A plugin that can be queued on a [`MarkdownItBuilder`].
///
/// Most implementations should be built from [`PluginSpec`]. Implement this
/// trait manually when a plugin needs custom installation code, typically by
/// returning [`PluginSpec::custom`].
pub trait Plugin {
    /// Convert this value into its installation specification.
    fn into_spec(self) -> PluginSpec;

    /// Install this plugin directly into an existing parser.
    fn install(self, md: &mut MarkdownIt)
    where
        Self: Sized,
    {
        self.into_spec().install(md);
    }
}

#[cfg(test)]
mod tests;
