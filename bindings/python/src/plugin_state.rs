use std::collections::{HashMap, HashSet};

use markdown_it::plugins::extra::heading_anchors::{
    EmptySlugPolicy,
    ExistingIdPolicy,
    is_heading,
    unique_slug,
};
use markdown_it::{Document, EditBatch, MarkdownIt, StructuralEvent};
use pyo3::prelude::*;
use pyo3::{PyAny, PyResult};

#[derive(Debug, Default)]
pub(crate) struct PluginState {
    enabled_plugin: HashSet<&'static str>,
    pub(crate) heading_anchors: Option<PythonHeadingAnchors>,
}

impl PluginState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert(&mut self, key: &'static str) -> bool {
        self.enabled_plugin.insert(key)
    }

    // makesure a plugins loaded only once
    pub(crate) fn add_once(
        &mut self,
        key: &'static str,
        md: &mut MarkdownIt,
        add: impl FnOnce(&mut MarkdownIt) -> PyResult<()>,
    ) -> PyResult<()> {
        if self.insert(key) {
            add(md)?;
        }
        Ok(())
    }
}

// --- heading anchors ---

#[derive(Debug)]
pub(crate) struct PythonHeadingAnchors {
    pub(crate) callback: Py<PyAny>,
    pub(crate) existing_id: ExistingIdPolicy,
    pub(crate) empty_slug: EmptySlugPolicy,
    pub(crate) prefix: Option<String>,
}

impl PythonHeadingAnchors {
    /// This mirrors `AddHeadingAnchors::run` but calls into Python for slug generation.
    pub(crate) fn apply(&self, py: Python<'_>, root: &mut Document) -> PyResult<()> {
        // 1. reserve IDs already present in the tree
        let mut used_ids = HashSet::new();
        for event in root.events(root.root()) {
            if matches!(event, StructuralEvent::Exit(_)) {
                continue;
            }
            let node = event.node();
            if is_heading(node) && matches!(self.existing_id, ExistingIdPolicy::Override) {
                continue;
            }
            used_ids.extend(
                node.attrs()
                    .iter()
                    .filter(|(name, _)| name == "id")
                    .map(|(_, value)| value.clone()),
            );
        }
        let mut next_suffix = HashMap::new();
        // 2. generate slugs via the Python callback
        let mut edits = EditBatch::new();
        for event in root.events(root.root()) {
            if matches!(event, StructuralEvent::Exit(_)) {
                continue;
            }
            let node = event.node();
            if !is_heading(node) {
                continue;
            }
            // handle existing id attribute
            if node.attrs().iter().any(|(name, _)| name == "id")
                && matches!(self.existing_id, ExistingIdPolicy::Keep)
            {
                continue;
            }
            let text: String = root
                .events(node.id())
                .filter_map(|event| {
                    if matches!(event, StructuralEvent::Exit(_)) {
                        return None;
                    }
                    let node = event.node();
                    if let Some(text) = node.cast::<markdown_it::Text>() {
                        Some(text.content.clone())
                    } else if let Some(text) = node.cast::<markdown_it::TextSpecial>() {
                        Some(text.content.clone())
                    } else if node.is::<markdown_it::plugins::cmark::inline::newline::Softbreak>() {
                        Some("\n".into())
                    } else {
                        None
                    }
                })
                .collect();
            // call Python callback: slug = strategy(text)
            let mut slug = self.callback.call1(py, (&text,))?.extract::<String>(py)?;
            // empty slug handling
            if slug.is_empty() {
                match &self.empty_slug {
                    EmptySlugPolicy::Skip => {
                        edits.remove_attribute(node.id(), "id");
                        continue;
                    }
                    EmptySlugPolicy::Use(fallback) => slug.clone_from(fallback),
                }
            }
            if slug.is_empty() {
                edits.remove_attribute(node.id(), "id");
                continue;
            }
            // apply prefix
            if let Some(prefix) = &self.prefix {
                slug.insert_str(0, prefix);
            }
            let slug = unique_slug(slug, &mut used_ids, &mut next_suffix);
            edits.set_attribute(node.id(), "id", slug);
        }
        edits.commit(root);
        Ok(())
    }
}
