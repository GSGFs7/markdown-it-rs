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
pub mod examples;
pub mod generics;
pub mod parser;
pub mod plugins;

pub use parser::main::MarkdownIt;
pub use parser::node::{HtmlAttribute, HtmlAttributes, Node, NodeValue};
pub use parser::render_options::RenderOptions;
pub use parser::renderer::Renderer;
pub use plugins::presets::{Preset, PresetConfig};
