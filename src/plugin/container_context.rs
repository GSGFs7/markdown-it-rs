//! Rendering contexts passed to inline container, inline pair, and block
//! container callbacks.

use std::fmt;

use crate::document::NodeId;
use crate::plugin::{Html, HtmlElement};
use crate::render::{RenderContext, Writer};

/// HTML builder passed to `render_html` callbacks.
///
/// Used by [`InlineContainerBuilder`](crate::plugin::InlineContainerBuilder),
/// [`InlinePairBuilder`](crate::plugin::InlinePairBuilder), and
/// [`BlockContainerBuilder`](crate::plugin::BlockContainerBuilder).
///
/// Children must be rendered explicitly with
/// [`render_children`](Self::render_children). The returned string is already
/// escaped HTML and can be inserted with [`raw`](Self::raw) or
/// [`HtmlElement::raw`].
pub struct ContainerHtml<'a, 'b> {
    html: &'a mut Html,
    context: &'a mut RenderContext<'b>,
    node: NodeId,
}

impl<'a, 'b> ContainerHtml<'a, 'b> {
    pub(in crate::plugin) fn new(
        html: &'a mut Html,
        context: &'a mut RenderContext<'b>,
        node: NodeId,
    ) -> Self {
        Self {
            html,
            context,
            node,
        }
    }

    /// Add a child element.
    pub fn element(&mut self, tag: impl Into<String>) -> HtmlElement<'_> {
        self.html.element(tag)
    }

    /// Append text. It is escaped when the fragment is rendered.
    pub fn text(&mut self, text: impl AsRef<str>) -> &mut Self {
        self.html.text(text);
        self
    }

    /// Append markup verbatim.
    pub fn raw(&mut self, html: impl AsRef<str>) -> &mut Self {
        self.html.raw(html);
        self
    }

    /// Render this container's children in the active output format.
    ///
    /// Returns an empty string for a leaf container with no children.
    pub fn render_children(&mut self) -> String {
        let mut output = Writer::default();
        self.context.render_children(self.node, &mut output);
        output.finish()
    }
}

/// Plain-text writer passed to `render_text` callbacks.
///
/// Used by [`InlineContainerBuilder`](crate::plugin::InlineContainerBuilder),
/// [`InlinePairBuilder`](crate::plugin::InlinePairBuilder), and
/// [`BlockContainerBuilder`](crate::plugin::BlockContainerBuilder).
pub struct ContainerText<'a, 'b> {
    output: &'a mut Writer,
    context: &'a mut RenderContext<'b>,
    node: NodeId,
}

impl<'a, 'b> ContainerText<'a, 'b> {
    pub(in crate::plugin) fn new(
        output: &'a mut Writer,
        context: &'a mut RenderContext<'b>,
        node: NodeId,
    ) -> Self {
        Self {
            output,
            context,
            node,
        }
    }

    /// Append a string to the output buffer.
    pub fn write_str(&mut self, value: &str) {
        self.output.write_str(value);
    }

    /// Append a char to the output buffer.
    pub fn write_char(&mut self, value: char) {
        self.output.write_char(value);
    }

    /// Append formatted output.
    ///
    /// # Panics
    ///
    /// Panics if a formatting implementation returns an error.
    pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) {
        self.output.write_fmt(args);
    }

    /// Render this container's children in the active output format.
    pub fn render_children(&mut self) {
        self.context.render_children(self.node, self.output);
    }
}

impl fmt::Write for ContainerText<'_, '_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        fmt::Write::write_str(self.output, value)
    }
}
