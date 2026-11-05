//! Registry of renderers, keyed by output format and node payload type.

use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

use super::context::{RenderShared, render_node};
use super::protocol::{FormatRenderers, TypedNodeRenderer};
use super::{NodeRenderer, RenderOptions, Writer};
use crate::common::extset::RenderExtSet;
use crate::document::{Document, NodeId, NodeValue};

/// Registry keyed by output format and concrete node payload type.
///
/// Adding the same pair again replaces the previous renderer and returns
/// `true`. This gives applications an explicit override mechanism.
#[derive(Default, Clone)]
pub struct RendererRegistry {
    formats: HashMap<String, FormatRenderers>,
}

impl fmt::Debug for RendererRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RendererRegistry")
            .field("formats", &self.formats.len())
            .field(
                "renderers",
                &self.formats.values().map(HashMap::len).sum::<usize>(),
            )
            .finish()
    }
}

impl RendererRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register or replace the renderer for `T` in `format`.
    pub fn add<T, R>(&mut self, format: impl Into<String>, renderer: R) -> bool
    where
        T: NodeValue,
        R: NodeRenderer<T>,
    {
        self.formats
            .entry(format.into())
            .or_default()
            .insert(
                TypeId::of::<T>(),
                std::sync::Arc::new(TypedNodeRenderer::<T, R> {
                    renderer,
                    marker: PhantomData,
                }),
            )
            .is_some()
    }

    pub fn contains<T: NodeValue>(&self, format: &str) -> bool {
        self.formats
            .get(format)
            .is_some_and(|renderers| renderers.contains_key(&TypeId::of::<T>()))
    }

    pub fn remove<T: NodeValue>(&mut self, format: &str) -> bool {
        let Some(renderers) = self.formats.get_mut(format) else {
            return false;
        };
        let removed = renderers.remove(&TypeId::of::<T>()).is_some();
        if renderers.is_empty() {
            self.formats.remove(format);
        }
        removed
    }

    /// Render a document.
    ///
    /// # Panics
    ///
    /// Panics if a leaf node has no renderer registered for `format`.
    pub fn render(&self, document: &Document, format: &str, options: &RenderOptions) -> String {
        self.render_subtree(document, document.root(), format, options)
    }

    /// Render a subtree using the same format registry.
    pub fn render_subtree(
        &self,
        document: &Document,
        root: NodeId,
        format: &str,
        options: &RenderOptions,
    ) -> String {
        let mut ext = RenderExtSet::new();
        let mut output = Writer::new();
        let shared = RenderShared::new(document, self.formats.get(format), format, options);
        render_node(&shared, &mut ext, root, &mut output);
        output.finish()
    }
}
