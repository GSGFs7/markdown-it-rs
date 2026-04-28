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
    options::require_no_options("linkify", options)?;

    #[cfg(feature = "linkify")]
    {
        state.add_once("linkify", md, |md| {
            markdown_it::plugins::extra::linkify::add(md);
            Ok(())
        })
    }

    #[cfg(not(feature = "linkify"))]
    {
        let _ = (md, state);
        Err(pyo3::exceptions::PyValueError::new_err(
            "linkify requires the linkify feature",
        ))
    }
}
