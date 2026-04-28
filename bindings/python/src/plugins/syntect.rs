use markdown_it::MarkdownIt;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::pyfunction;
use pyo3::types::PyDict;

#[cfg(feature = "syntect")]
use crate::options;
use crate::plugin_state::PluginState;

#[pyfunction]
pub(crate) fn available_syntax_themes() -> Vec<String> {
    #[cfg(feature = "syntect")]
    {
        markdown_it::plugins::extra::syntect::available_themes()
    }

    #[cfg(not(feature = "syntect"))]
    {
        Vec::new()
    }
}

pub(crate) fn enable(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    #[cfg(feature = "syntect")]
    {
        options::validate_options(
            "syntect",
            options,
            &["theme", "classed", "syntax_theme", "syntax_classed"],
        )?;

        let theme = options::get_string(options, "theme")?
            .or(options::get_string(options, "syntax_theme")?);
        let classed = options::get_bool(options, "classed")?
            .or(options::get_bool(options, "syntax_classed")?)
            .unwrap_or(false);

        enable_with_config(md, state, theme.as_deref(), classed)
    }

    #[cfg(not(feature = "syntect"))]
    {
        let _ = (md, state, options);
        Err(PyValueError::new_err(
            "syntax highlighting requires the syntect feature",
        ))
    }
}

pub(crate) fn enable_with_config(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    theme: Option<&str>,
    classed: bool,
) -> PyResult<()> {
    #[cfg(feature = "syntect")]
    {
        if state.insert("syntect") {
            markdown_it::plugins::extra::syntect::add(md);
        }
        if let Some(theme) = theme {
            if !markdown_it::plugins::extra::syntect::available_themes()
                .iter()
                .any(|available_theme| available_theme == theme)
            {
                return Err(PyValueError::new_err(format!(
                    "unknown syntect theme: {theme}"
                )));
            }
            markdown_it::plugins::extra::syntect::set_theme(md, theme);
        }
        if classed {
            markdown_it::plugins::extra::syntect::set_to_classed(md);
        }

        Ok(())
    }

    #[cfg(not(feature = "syntect"))]
    {
        let _ = (md, state, theme, classed);
        Err(PyValueError::new_err(
            "syntax highlighting requires the syntect feature",
        ))
    }
}
