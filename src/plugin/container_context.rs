//! Rendering contexts passed to inline container, inline pair, and block
//! container callbacks.

use std::fmt;

use crate::document::{Document, NodeId};
use crate::plugin::{Html, HtmlElement, InlineContainerPart};
use crate::render::{RenderContext, Writer};

/// HTML builder passed to `render_html` callbacks.
///
/// Used by [`InlineContainerBuilder`](crate::plugin::InlineContainerBuilder),
/// [`InlinePairBuilder`](crate::plugin::InlinePairBuilder), and
/// [`BlockContainerBuilder`](crate::plugin::BlockContainerBuilder).
///
/// Children are not rendered automatically: call
/// [`render_children`](Self::render_children) for the whole body or
/// [`render_part`](Self::render_part) for one inline-container part. The
/// returned string is already escaped HTML and can be inserted with
/// [`raw`](Self::raw) or [`HtmlElement::raw`].
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

    /// The document being rendered.
    pub fn document(&self) -> &'b Document {
        self.context.document()
    }

    /// ID of the container or pair node currently being rendered.
    pub fn node_id(&self) -> NodeId {
        self.node
    }

    /// Original Markdown source covered by this node.
    ///
    /// Available when the `sourcemap` Cargo feature is enabled. Returns `None`
    /// if the node has no source map, for example after a transform replaced it.
    #[cfg(feature = "sourcemap")]
    pub fn source_text(&self) -> Option<&'b str> {
        let document = self.context.document();
        let (start, end) = document.node(self.node).srcmap()?.get_byte_offsets();
        document.source().get(start..end)
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

    /// Render one inline-container part by zero-based index.
    ///
    /// Use this when the output interleaves parts. Unlike
    /// [`render_children`](Self::render_children), it renders no other parts.
    /// Missing indices and non-inline containers produce an empty string. The
    /// returned HTML is already escaped and can be inserted with
    /// [`raw`](Self::raw).
    pub fn render_part(&mut self, index: usize) -> String {
        let mut output = Writer::default();
        if let Some(&part) = self.context.document().children(self.node).get(index) {
            if self
                .context
                .document()
                .value::<InlineContainerPart>(part)
                .is_some()
            {
                self.context.render_children(part, &mut output);
            }
        }
        output.finish()
    }

    /// Render the whole body in the active output format.
    ///
    /// For inline containers this renders every part in order; for pairs and
    /// block containers it renders their normal children. Returns an empty
    /// string when there is nothing to render. Use
    /// [`render_part`](Self::render_part) to render one part.
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
///
/// Children are not rendered automatically: call
/// [`render_children`](Self::render_children) for the whole body or
/// [`render_part`](Self::render_part) for one inline-container part.
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

    /// The document being rendered.
    pub fn document(&self) -> &'b Document {
        self.context.document()
    }

    /// ID of the container or pair node currently being rendered.
    pub fn node_id(&self) -> NodeId {
        self.node
    }

    /// Original Markdown source covered by this node.
    ///
    /// See [`ContainerHtml::source_text`].
    #[cfg(feature = "sourcemap")]
    pub fn source_text(&self) -> Option<&'b str> {
        let document = self.context.document();
        let (start, end) = document.node(self.node).srcmap()?.get_byte_offsets();
        document.source().get(start..end)
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

    /// Render one inline-container part by zero-based index.
    ///
    /// See [`ContainerHtml::render_part`]. Missing indices and non-inline
    /// containers produce no output.
    pub fn render_part(&mut self, index: usize) {
        if let Some(&part) = self.context.document().children(self.node).get(index) {
            if self
                .context
                .document()
                .value::<InlineContainerPart>(part)
                .is_some()
            {
                self.context.render_children(part, self.output);
            }
        }
    }

    /// Render the whole body in the active output format.
    ///
    /// For inline containers this renders every part in order. Use
    /// [`render_part`](Self::render_part) to render one part; see
    /// [`ContainerHtml::render_children`] for details.
    pub fn render_children(&mut self) {
        self.context.render_children(self.node, self.output);
    }
}

impl fmt::Write for ContainerText<'_, '_> {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        fmt::Write::write_str(self.output, value)
    }
}
