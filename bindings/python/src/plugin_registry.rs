use markdown_it::MarkdownIt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::plugin_state::PluginState;
use crate::plugins;

pub(crate) fn enable(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let name = plugin.replace('_', "-").to_ascii_lowercase();
    match name.as_str() {
        "commonmark" | "cmark" => plugins::builtin::commonmark(md, state, plugin, options),
        "html" => plugins::builtin::html(md, state, plugin, options),
        "tables" | "table" => plugins::builtin::tables(md, state, plugin, options),
        "strikethrough" => plugins::builtin::strikethrough(md, state, plugin, options),
        "beautify-links" => plugins::builtin::beautify_links(md, state, plugin, options),
        "frontmatter" | "front-matter" => plugins::frontmatter::enable(md, state, options),
        "heading-anchors" | "heading-anchor" => {
            plugins::builtin::heading_anchors(md, state, plugin, options)
        }
        "linkify" => plugins::linkify::enable(md, state, options),
        "math" => plugins::builtin::math(md, state, plugin, options),
        "smartquotes" => plugins::builtin::smartquotes(md, state, plugin, options),
        "sourcepos" | "source-pos" => plugins::builtin::sourcepos(md, state, plugin, options),
        "typographer" => plugins::typographer::enable(md, state, options),
        "syntect" | "syntax-highlighting" => plugins::syntect::enable(md, state, options),
        _ => Err(PyValueError::new_err(format!("unknown plugin: {plugin}"))),
    }
}
