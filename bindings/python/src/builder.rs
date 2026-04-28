use markdown_it::MarkdownIt;
use pyo3::PyResult;

use crate::plugin_state::PluginState;
use crate::plugins;

pub(crate) struct BuiltMarkdownIt {
    pub(crate) inner: MarkdownIt,
    pub(crate) plugins: PluginState,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build(
    enable_html: bool,
    enable_linkify: bool,
    enable_math: bool,
    enable_frontmatter: bool,
    enable_typographer: bool,
    enable_sourcepos: bool,
    enable_heading_anchors: bool,
    enable_syntax_highlighting: bool,
    syntax_theme: Option<&str>,
    syntax_classed: bool,
) -> PyResult<BuiltMarkdownIt> {
    // avoid warning when syntect feature is disabled
    #[cfg(not(feature = "syntect"))]
    let _ = (syntax_theme, syntax_classed);

    let mut inner = MarkdownIt::new();
    let mut state = PluginState::new();

    if enable_frontmatter {
        plugins::frontmatter::enable_with_max_lines(&mut inner, &mut state, None)?;
    }
    plugins::builtin::commonmark(&mut inner, &mut state, "commonmark", None)?;
    plugins::builtin::tables(&mut inner, &mut state, "tables", None)?;
    plugins::builtin::strikethrough(&mut inner, &mut state, "strikethrough", None)?;
    plugins::builtin::beautify_links(&mut inner, &mut state, "beautify-links", None)?;
    if enable_heading_anchors {
        plugins::builtin::heading_anchors(&mut inner, &mut state, "heading-anchors", None)?;
    }
    if enable_html {
        plugins::builtin::html(&mut inner, &mut state, "html", None)?;
    }
    if enable_linkify {
        plugins::linkify::enable(&mut inner, &mut state, None)?;
    }
    if enable_math {
        plugins::builtin::math(&mut inner, &mut state, "math", None)?;
    }
    if enable_typographer {
        plugins::typographer::enable(&mut inner, &mut state, None)?;
    }
    if enable_sourcepos {
        plugins::builtin::sourcepos(&mut inner, &mut state, "sourcepos", None)?;
    }
    if enable_syntax_highlighting {
        plugins::syntect::enable_with_config(&mut inner, &mut state, syntax_theme, syntax_classed)?;
    }

    Ok(BuiltMarkdownIt {
        inner,
        plugins: state,
    })
}
