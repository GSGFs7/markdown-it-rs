// for bragging rights
#![forbid(unsafe_code)]
//
// useful asserts that's off by default
#![warn(clippy::manual_assert)]
#![warn(clippy::semicolon_if_nothing_returned)]
//
// these are often intentionally not collapsed for readability
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_match)]

pub mod common;
pub mod document;
pub mod examples;
pub mod links;
mod markdown_it;
pub mod parser;
pub mod plugin;
pub mod plugins;
pub mod render;

pub use document::edit::EditBatch;
pub use document::text::{TextBoundary, TextClassifier, TextEvent, TextEvents, TextProjectionKind};
pub use document::{
    Document,
    HtmlAttribute,
    HtmlAttributes,
    Node,
    NodeDraft,
    NodeId,
    NodeRef,
    NodeValue,
    Root,
    StructuralEvent,
    StructuralEvents,
    Text,
    TextSpecial,
};
pub use markdown_it::MarkdownIt;
pub use parser::block::BlockState;
pub use parser::inline::InlineState;
pub use plugin::{
    BlockLeafBuilder,
    BlockPluginState,
    Html,
    HtmlElement,
    InlineLeafBuilder,
    InlinePairBuilder,
    InlinePlugin,
    InlinePluginState,
    LeafRenderer,
    LeafRule,
    MarkdownItBuilder,
    PairDelimiter,
    Plugin,
    PluginSpec,
    WithRenderer,
};
pub use plugins::presets::{Preset, PresetConfig};
pub use render::{NodeRenderer, RenderContext, RenderOptions, RendererRegistry, Writer};
