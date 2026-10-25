use super::spec::install_plugins;
use super::{Plugin, PluginSpec};
use crate::MarkdownIt;
use crate::plugins::presets::{Preset, PresetConfig};

/// A parser configured with [`MarkdownIt::builder`].
#[derive(Debug)]
pub struct MarkdownItBuilder {
    md: MarkdownIt,
    plugins: Vec<PluginSpec>,
}

impl MarkdownItBuilder {
    /// Start from the markdown-it default preset.
    pub fn new() -> Self {
        Self {
            md: MarkdownIt::new(),
            plugins: Vec::new(),
        }
    }

    /// Start from an empty parser. Equivalent to `with_preset(|_| {})`.
    pub fn empty() -> Self {
        Self {
            md: MarkdownIt::empty(),
            plugins: Vec::new(),
        }
    }

    /// Start from a built-in or custom preset.
    pub fn with_preset(preset: impl PresetConfig) -> Self {
        Self {
            md: MarkdownIt::with_preset(preset),
            plugins: Vec::new(),
        }
    }

    /// Use a built-in preset as the base parser.
    pub fn preset(mut self, preset: Preset) -> Self {
        self.md = MarkdownIt::with_preset(preset);
        self
    }

    /// Queue a plugin for installation.
    pub fn plugin(mut self, plugin: impl Plugin) -> Self {
        self.plugins.push(plugin.into_spec());
        self
    }

    /// Validate plugin identities, callbacks, dependencies and rule chains,
    /// install all queued plugins, then compile the rule tables and text
    /// scanner before parsing begins.
    ///
    /// # Panics
    ///
    /// Panics on duplicate plugin names, empty names or rule names, missing
    /// callbacks, missing dependencies, dependency cycles, ordering against
    /// already-installed plugins, or invalid low-level rule chains.
    pub fn build(mut self) -> MarkdownIt {
        let plugins = self
            .plugins
            .into_iter()
            .flat_map(PluginSpec::expand)
            .collect();
        install_plugins(&mut self.md, plugins);
        self.md.validate_rules();
        let _ = self.md.block.rules();
        let _ = self.md.inline.rules();
        let _ = self.md.inline.text_length("", 0, 0);
        self.md
    }
}

impl Default for MarkdownItBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownIt {
    /// Start building a parser with high-level plugins.
    ///
    /// Starts from the markdown-it default preset. Use
    /// [`MarkdownItBuilder::empty`] or [`MarkdownItBuilder::with_preset`]
    /// to choose a different base.
    pub fn builder() -> MarkdownItBuilder {
        MarkdownItBuilder::new()
    }
}
