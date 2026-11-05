//! Recursive render engine: shared state, [`RenderContext`], and tree traversal.

use super::protocol::FormatRenderers;
use super::{RenderOptions, Writer};
use crate::common::extset::RenderExtSet;
use crate::document::{Document, NodeId};

pub(super) struct RenderShared<'a> {
    document: &'a Document,
    renderers: Option<&'a FormatRenderers>,
    format: &'a str,
    options: &'a RenderOptions,
}

impl<'a> RenderShared<'a> {
    pub(super) fn new(
        document: &'a Document,
        renderers: Option<&'a FormatRenderers>,
        format: &'a str,
        options: &'a RenderOptions,
    ) -> Self {
        Self {
            document,
            renderers,
            format,
            options,
        }
    }
}

/// Recursive services shared by node renderers for one render operation.
pub struct RenderContext<'a> {
    shared: &'a RenderShared<'a>,
    ext: &'a mut RenderExtSet,
}

impl<'a> RenderContext<'a> {
    /// The document being rendered.
    pub fn document(&self) -> &'a Document {
        self.shared.document
    }

    /// The active output format, e.g. `"html"`.
    pub fn format(&self) -> &str {
        self.shared.format
    }

    /// Render options for the active format.
    pub fn options(&self) -> &RenderOptions {
        self.shared.options
    }

    /// Extension set for renderer-specific state.
    pub fn ext(&mut self) -> &mut RenderExtSet {
        self.ext
    }

    /// Write one line ending unless the output is already at the start of a line.
    pub fn cr(&mut self, output: &mut Writer) {
        output.cr();
    }

    /// Render one node's shell using its registered renderer.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// has no renderer for the active format, or if a renderer or formatting
    /// implementation panics.
    pub fn render_node(&mut self, node: NodeId, output: &mut Writer) {
        render_node(self.shared, self.ext, node, output);
    }

    /// Render all direct children of `node` in order.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not a valid [`NodeId`] in this document, if a leaf
    /// descendant has no renderer for the active format, or if a renderer or
    /// formatting implementation panics.
    pub fn render_children(&mut self, node: NodeId, output: &mut Writer) {
        render_children_guarded(self.shared, self.ext, node, output);
    }
}

/// Render one node, transparently descending through unknown containers.
pub(super) fn render_node(
    shared: &RenderShared<'_>,
    ext: &mut RenderExtSet,
    id: NodeId,
    output: &mut Writer,
) {
    let node = shared.document.node(id);
    let Some(renderer) = shared
        .renderers
        .and_then(|renderers| renderers.get(&node.type_id()))
    else {
        // leaf but no renderer
        assert!(
            !node.children().is_empty(),
            "no {:?} renderer registered for leaf {} at {:?}",
            shared.format,
            node.name(),
            id,
        );

        // transparent downward traversal
        render_children_guarded(shared, ext, id, output);
        return;
    };

    // render the shell (<h1>,<em>,...)
    let mut context = RenderContext { shared, ext };
    renderer.render(node, &mut context, output);
}

/// Render every direct child of `node`, growing the stack for deep trees.
fn render_children_guarded(
    shared: &RenderShared<'_>,
    ext: &mut RenderExtSet,
    node: NodeId,
    output: &mut Writer,
) {
    for &child in shared.document.children(node) {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            render_node(shared, ext, child, output);
        });
    }
}
