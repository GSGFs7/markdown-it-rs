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
    options::validate_options("frontmatter", options, &["max_lines"])?;
    let max_lines = options::get_usize(options, "max_lines")?;
    enable_with_max_lines(md, state, max_lines)
}

pub(crate) fn enable_with_max_lines(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    max_lines: Option<usize>,
) -> PyResult<()> {
    if state.insert("frontmatter") {
        markdown_it::plugins::extra::front_matter::add_with_max_lines(
            md,
            max_lines.unwrap_or(markdown_it::plugins::extra::front_matter::DEFAULT_MAX_LINES),
        );
    } else if let Some(max_lines) = max_lines {
        markdown_it::plugins::extra::front_matter::set_max_lines(md, max_lines);
    }

    Ok(())
}
