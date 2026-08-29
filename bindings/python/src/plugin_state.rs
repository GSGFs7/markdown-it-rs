use std::collections::{HashMap, HashSet};

use markdown_it::plugins::extra::heading_anchors::{
    EmptySlugPolicy,
    ExistingIdPolicy,
    is_heading,
    unique_slug,
};
use markdown_it::{MarkdownIt, Node};
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
    pub(crate) fn apply(&self, py: Python<'_>, root: &mut Node) -> PyResult<()> {
        // 1. reserve IDs already present in the tree
        let mut used_ids = HashSet::new();
        root.walk(|node, _| {
            if is_heading(node) && matches!(self.existing_id, ExistingIdPolicy::Override) {
                return;
            }
            used_ids.extend(
                node.attrs
                    .iter()
                    .filter(|(name, _)| *name == "id")
                    .map(|(_, value)| value.clone()),
            );
        });
        let mut next_suffix = HashMap::new();

        // 2. generate slugs via the Python callback
        let mut first_error: Option<PyErr> = None;
        root.walk_mut(|node, _| {
            if first_error.is_some() || !is_heading(node) {
                return;
            }

            // handle existing id attribute
            if node.attrs.iter().any(|(name, _)| *name == "id") {
                match self.existing_id {
                    ExistingIdPolicy::Keep => return,
                    ExistingIdPolicy::Override => {
                        node.attrs.retain(|(name, _)| *name != "id");
                    }
                }
            }

            // call Python callback: slug = strategy(text)
            let text = node.collect_text();
            let slug_result = self
                .callback
                .call1(py, (&text,))
                .and_then(|obj| obj.extract::<String>(py));

            let mut slug = match slug_result {
                Ok(s) => s,
                Err(e) => {
                    first_error = Some(e);
                    return;
                }
            };

            // empty slug handling
            if slug.is_empty() {
                match &self.empty_slug {
                    EmptySlugPolicy::Skip => return,
                    EmptySlugPolicy::Use(fallback) => slug.clone_from(fallback),
                }
            }
            if slug.is_empty() {
                return;
            }

            // apply prefix
            if let Some(prefix) = &self.prefix {
                slug.insert_str(0, prefix);
            }

            let slug = unique_slug(slug, &mut used_ids, &mut next_suffix);
            node.attrs.push(("id".into(), slug));
        });

        if let Some(err) = first_error {
            return Err(err);
        }

        Ok(())
    }
}
