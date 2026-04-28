use markdown_it::MarkdownIt;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::options;
use crate::plugin_state::PluginState;

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

pub(crate) fn heading_anchors(
    md: &mut MarkdownIt,
    state: &mut PluginState,
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    options::require_no_options(plugin, options)?;
    state.add_once("heading-anchors", md, |md| {
        markdown_it::plugins::extra::heading_anchors::add(
            md,
            markdown_it::plugins::extra::heading_anchors::simple_slugify_fn,
        );
        Ok(())
    })
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
