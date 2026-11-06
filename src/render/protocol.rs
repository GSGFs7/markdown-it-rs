//! Renderer protocol: the public [`NodeRenderer`] trait and its type-erased form.

use std::any::TypeId;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;

use super::{RenderContext, Writer};
use crate::common::typekey::TypeIdHasher;
use crate::document::{Node, NodeValue};

/// Renderer for one payload type in one output format.
///
/// ```
/// use markdown_it::document::{Document, Node};
/// use markdown_it::render::{NodeRenderer, RenderContext, Writer};
/// use markdown_it::MarkdownIt;
///
/// #[derive(Debug)]
/// struct Badge(&'static str);
///
/// struct BadgeRenderer;
/// impl NodeRenderer<Badge> for BadgeRenderer {
///     fn render(
///         &self,
///         _: &Node,
///         badge: &Badge,
///         _: &mut RenderContext<'_>,
///         output: &mut Writer,
///     ) {
///         output.write_str(badge.0);
///     }
/// }
///
/// let mut md = MarkdownIt::empty();
/// md.add_renderer::<Badge, _>("html", BadgeRenderer);
/// let document = Document::new("", Badge("new"));
/// assert_eq!(md.render_document(&document, "html"), "new");
/// ```
pub trait NodeRenderer<T: NodeValue>: Send + Sync + 'static {
    fn render(&self, node: &Node, value: &T, context: &mut RenderContext<'_>, output: &mut Writer);
}

pub(super) trait ErasedNodeRenderer: Send + Sync {
    fn render(&self, node: &Node, context: &mut RenderContext<'_>, output: &mut Writer);
}

// --- machinery ---

pub(super) type FormatRenderers =
    HashMap<TypeId, std::sync::Arc<dyn ErasedNodeRenderer>, BuildHasherDefault<TypeIdHasher>>;

pub(super) struct TypedNodeRenderer<T, R> {
    pub(super) renderer: R,
    pub(super) marker: PhantomData<fn() -> T>,
}

impl<T, R> ErasedNodeRenderer for TypedNodeRenderer<T, R>
where
    T: NodeValue,
    R: NodeRenderer<T>,
{
    fn render(&self, node: &Node, context: &mut RenderContext<'_>, output: &mut Writer) {
        let value = node
            .cast::<T>()
            .expect("renderer registry type and payload type must agree");
        self.renderer.render(node, value, context, output);
    }
}
