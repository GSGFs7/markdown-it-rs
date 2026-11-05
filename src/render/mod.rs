//! Format-specific rendering for documents.
//!
//! A [`RendererRegistry`] maps an output format (for example `"html"` or
//! `"text"`) and a node payload type to a [`NodeRenderer`]. Rendering starts at
//! [`RenderContext`] and writes into a [`Writer`].
//!
//! Built-in renderers live in the private `builtin` module; the HTML fragments
//! they emit are produced with the helpers in the private `html` module.

mod builtin;
mod context;
mod html;
mod options;
mod protocol;
mod registry;
mod writer;

#[cfg(test)]
mod tests;

pub(crate) use builtin::{
    DebugTreeRenderer,
    EmptyRenderer,
    HtmlBlockElementRenderer,
    HtmlInlineElementRenderer,
    HtmlTextRenderer,
    PlainTextBlockRenderer,
    PlainTextBreakRenderer,
    PlainTextRenderer,
    TransparentRenderer,
};
pub use context::RenderContext;
pub(crate) use html::{
    render_html_link,
    write_html_attrs_with,
    write_html_close,
    write_html_open,
    write_html_self_close,
    write_html_text,
};
pub use options::RenderOptions;
pub use protocol::NodeRenderer;
pub use registry::RendererRegistry;
pub use writer::Writer;
