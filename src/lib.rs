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
pub mod plugins;
pub mod render;

pub use document::edit::EditBatch;
pub use document::text::{
    TextBoundary,
    TextClassifier,
    TextEvent,
    TextEvents,
    TextProjection,
    TextProjectionKind,
};
pub use document::transform::{DocumentTransform, DocumentTransformRegistry, TransformRuleBuilder};
pub use document::{
    Document,
    DocumentNode,
    HtmlAttribute,
    HtmlAttributes,
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
pub use parser::block::DocumentBlockState;
pub use parser::inline::DocumentInlineState;
pub use plugins::presets::{Preset, PresetConfig};
pub use render::{
    DocumentNodeRenderer,
    DocumentRenderContext,
    DocumentRendererRegistry,
    DocumentWriter,
    RenderOptions,
};
