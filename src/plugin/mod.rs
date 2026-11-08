//! High-level plugin builders.
//!
//! A [`PluginSpec`] groups syntax rules and transforms, resolves dependency
//! ordering, and installs them into a [`MarkdownIt`](crate::MarkdownIt).
//!
//! ## Adding syntax
//!
//! Configure syntax on a [`PluginSpec`] with:
//!
//! - [`PluginSpec::inline_leaf`], [`PluginSpec::inline_container`],
//!   [`PluginSpec::inline_pair`]
//! - [`PluginSpec::block_leaf`], [`PluginSpec::block_container`]
//! - [`PluginSpec::transform`]
//!
//! Builders render HTML directly with `.render_html(...)`, or accept a
//! low-level [`NodeRenderer`](crate::render::NodeRenderer) via
//! `.render_with(format, renderer)`.
//!
//! ## Naming and ordering
//!
//! Rule names are unique within a plugin across all rule kinds, but scoped to
//! that plugin, so other plugins may reuse them. A same-name child merged with
//! [`PluginSpec::with`] shares the parent's namespace; otherwise groups keep
//! child identities.
//!
//! Names are logical, not low-level rule boundaries: block syntax shares one
//! dispatcher and pairs share one scanner per marker. Only transforms expose
//! ordering marks ([`transform_rule_mark`], [`transform_plugin_mark`]); other
//! rules have no per-name ordering, lookup, or removal. Mark encodings are
//! internal.
//!
//! ## Renderers
//!
//! A payload type has one renderer per output format per parser. Repeated
//! registrations for the same type and format use the last renderer, matching
//! [`MarkdownIt::add_renderer`](crate::MarkdownIt::add_renderer). Register the
//! same payload type from multiple plugins only if the rendering is identical;
//! use a newtype otherwise.
//!
//! ## Example
//!
//! An inline leaf:
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
    InlinePluginState,
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
