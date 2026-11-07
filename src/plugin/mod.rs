//! High-level plugin builders.
//!
//! There are two public plugin concepts:
//!
//! - [`InlinePlugin`]: a static inline leaf that parses and renders itself.
//! - [`PluginSpec`]: groups leaves, adds dependency ordering and installs them.
//!
//! Configured syntax is added to a [`PluginSpec`] with
//! [`PluginSpec::inline_leaf`], [`PluginSpec::inline_container`],
//! [`PluginSpec::block_leaf`], [`PluginSpec::block_container`],
//! [`PluginSpec::inline_pair`] or [`PluginSpec::transform`]. Syntax builders
//! accept low-level [`NodeRenderer`](crate::render::NodeRenderer)
//! implementations with `.render_with(format, renderer)`.
//!
//! Rule names are local to their plugin and unique across all rule kinds;
//! other plugins may reuse them. Groups keep child identities, and a same-name
//! child merged with [`PluginSpec::with`] shares the parent's namespace.
//!
//! Logical names do not imply separate low-level rules: block syntax shares a
//! dispatcher and pairs share a scanner per marker. Only transforms expose
//! named ordering marks through [`transform_rule_mark`] and
//! [`transform_plugin_mark`]; other syntax rules have no per-name ordering,
//! lookup or removal. Mark encodings are internal.
//!
//! A payload type has one renderer per output format per parser. Register a
//! second plugin for the same payload type only when it uses the same rendering;
//! use a newtype when the rendering differs. Repeated registrations for a type
//! and format use the last renderer, matching [`MarkdownIt::add_renderer`](crate::MarkdownIt::add_renderer).
//!
//! Static leaf:
//!
//! ```
//! use markdown_it::plugin::{Html, InlinePlugin, InlinePluginState};
//! use markdown_it::MarkdownIt;
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
//! use markdown_it::plugin::{Html, PluginSpec};
//! use markdown_it::MarkdownIt;
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
mod container_context;
mod html;
mod identity;
mod inline;
mod renderer;
mod spec;
mod transform;

pub use self::block::{
    BlockContainerBuilder,
    BlockContainerParse,
    BlockContainerState,
    BlockLeafBuilder,
    BlockPluginState,
};
pub use self::builder::MarkdownItBuilder;
pub use self::container_context::{ContainerHtml, ContainerText};
pub use self::html::{Html, HtmlElement};
pub use self::inline::{
    InlineContainerBuilder,
    InlineContainerState,
    InlineLeafBuilder,
    InlinePairBuilder,
    InlinePlugin,
    InlinePluginState,
    LeafRenderer,
    LeafRule,
    PairDelimiter,
};
pub use self::renderer::WithRenderer;
pub use self::spec::PluginSpec;
pub use self::transform::{transform_plugin_mark, transform_rule_mark};

/// A plugin that can be queued on a [`MarkdownItBuilder`].
///
/// Most implementations should be built from [`PluginSpec`]. Implement this
/// trait manually when a plugin needs custom installation code, typically by
/// returning [`PluginSpec::custom`].
pub trait Plugin {
    /// Convert this value into its installation specification.
    fn into_spec(self) -> PluginSpec;

    /// Install this plugin directly into an existing parser.
    ///
    /// This delegates to [`PluginSpec::install`], including its validation and
    /// final rule-chain compilation behavior.
    fn install(self, md: &mut crate::MarkdownIt)
    where
        Self: Sized,
    {
        self.into_spec().install(md);
    }
}

#[cfg(test)]
mod tests;
