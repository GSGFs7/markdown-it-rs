use std::collections::HashSet;

use markdown_it::MarkdownIt;
use pyo3::PyResult;

#[derive(Debug, Default)]
pub(crate) struct PluginState {
    enabled_plugin: HashSet<&'static str>,
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
