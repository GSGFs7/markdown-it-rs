//! High-level plugin builders.
//!
//! There are two public plugin concepts:
//!
//! - [`InlinePlugin`]: a static leaf node that knows how to parse and render itself.
//! - [`PluginSpec`]: groups leaves, adds dependency ordering and installs them.
//!
//! Configured leaves are added directly to a [`PluginSpec`] with
//! [`PluginSpec::inline_leaf`].
//!
//! A payload type has one HTML renderer per parser. Register a second plugin for
//! the same payload type only when it uses the same rendering; use a newtype when
//! the rendering differs.
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
//!     const RULE: &'static str = "rocket";
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

// TODO: block rule

mod builder;
mod html;
mod inline;
mod spec;
mod state;

pub use builder::MarkdownItBuilder;
pub use html::{Html, HtmlElement};
pub use inline::{InlineLeafBuilder, InlinePlugin, LeafRenderer, LeafRule};
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
