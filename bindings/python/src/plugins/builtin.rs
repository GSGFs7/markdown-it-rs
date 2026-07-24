use markdown_it::MarkdownIt;
use markdown_it::plugins::extra::heading_anchors::{
    AddHeadingAnchors,
    EmptySlugPolicy,
    ExistingIdPolicy,
    HeadingAnchorsOptions,
    SlugStrategy,
    add_with_options,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::options;
use crate::plugin_state::{PluginState, PythonHeadingAnchors};

pub(crate) fn commonmark(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("commonmark", md, |md| {
        markdown_it::plugins::cmark::add(md);
        Ok(())
    })
}

pub(crate) fn html(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("html", md, |md| {
        markdown_it::plugins::html::add(md);
        Ok(())
    })
}

pub(crate) fn tables(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("tables", md, |md| {
        markdown_it::plugins::extra::tables::add(md);
        Ok(())
    })
}

pub(crate) fn strikethrough(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("strikethrough", md, |md| {
        markdown_it::plugins::extra::strikethrough::add(md);
        Ok(())
    })
}

pub(crate) fn mark(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("mark", md, |md| {
        markdown_it::plugins::extra::mark::add(md);
        Ok(())
    })
}

pub(crate) fn beautify_links(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("beautify-links", md, |md| {
        markdown_it::plugins::extra::beautify_links::add(md);
        Ok(())
    })
}

pub(crate) fn directives(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("directives", md, |md| {
        markdown_it::plugins::directives::add(md);
        Ok(())
    })
}

pub(crate) fn tasklist(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("tasklist", md, |md| {
        markdown_it::plugins::extra::tasklist::add(md);
        Ok(())
    })
}

pub(crate) fn footnote(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("footnote", md, |md| {
        markdown_it::plugins::extra::footnote::add(md);
        Ok(())
    })
}

pub(crate) fn heading_anchors(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::validate_options(
        plugin,
        options,
        &["strategy", "existing_id", "empty_slug", "prefix"],
    )?;

    let strategy = options
        .and_then(|opts| opts.get_item("strategy").transpose())
        .transpose()?;
    let existing_id = match options::get_string(options, "existing_id")?.as_deref() {
        None | Some("keep") => ExistingIdPolicy::Keep,
        Some("override") => ExistingIdPolicy::Override,
        Some(policy) => {
            return Err(PyValueError::new_err(format!(
                "unknown existing heading ID policy: {policy}"
            )));
        }
    };
    let empty_slug = options::get_optional_string(options, "empty_slug")?
        .map_or(EmptySlugPolicy::Skip, EmptySlugPolicy::Use);
    let prefix = options::get_optional_string(options, "prefix")?;

    match strategy {
        // use default
        None => {
            state.heading_anchors = None;
            md.remove_rule::<AddHeadingAnchors>();
            add_with_options(
                md,
                HeadingAnchorsOptions {
                    strategy: SlugStrategy::default(),
                    existing_id,
                    empty_slug,
                    prefix,
                },
            );
        }
        // use custom
        Some(ref value) if value.is_callable() => {
            state.heading_anchors = Some(PythonHeadingAnchors {
                callback: value.clone().unbind(),
                existing_id,
                empty_slug,
                prefix,
            });
            md.remove_rule::<AddHeadingAnchors>();
        }
        // use builtins
        Some(value) => {
            let value_str = value.extract::<String>()?;
            let strategy = match value_str.as_str() {
                "simple" => SlugStrategy::Simple,
                "github" => SlugStrategy::GitHub,
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "unknown heading anchor strategy: {value_str}"
                    )));
                }
            };
            let options = HeadingAnchorsOptions {
                strategy,
                existing_id,
                empty_slug,
                prefix,
            };

            state.heading_anchors = None;
            md.remove_rule::<AddHeadingAnchors>();
            add_with_options(md, options);
        }
    };

    Ok(())
}

pub(crate) fn math(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("math", md, |md| {
        markdown_it::plugins::extra::math::add(md);
        Ok(())
    })
}

pub(crate) fn smartquotes(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("smartquotes", md, |md| {
        markdown_it::plugins::extra::smartquotes::add(md);
        Ok(())
    })
}

pub(crate) fn sourcepos(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("sourcepos", md, |md| {
        markdown_it::plugins::sourcepos::add(md);
        Ok(())
    })
}
