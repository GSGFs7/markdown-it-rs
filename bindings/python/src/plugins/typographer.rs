use markdown_it::MarkdownIt;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::options;
use crate::plugin_state::PluginState;

pub(crate) fn enable(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options("typographer", options)?;

    if state.insert("typographer") {
        markdown_it::plugins::extra::typographer::add(md);
    }
    if state.insert("smartquotes") {
        markdown_it::plugins::extra::smartquotes::add(md);
    }

    Ok(())
}
