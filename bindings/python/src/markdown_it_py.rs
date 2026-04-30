use std::cell::RefCell;

use markdown_it::MarkdownIt;
use markdown_it::parser::core::Root;
use markdown_it::plugins::extra::front_matter::FrontMatter;
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple, PyTupleMethods};

use crate::ast::PyAst;
use crate::builder::build;
use crate::plugin_registry;
use crate::plugin_state::PluginState;
use crate::types::{PyFrontMatter, PyMarkdownOutput};

#[pyclass(name = "MarkdownIt", dict)]
pub(crate) struct PyMarkdownIt {
    inner: MarkdownIt,
    plugins: PluginState,
}

#[pymethods]
impl PyMarkdownIt {
    #[new]
    #[pyo3(signature=(
        *,
        html = false,
        linkify = false,
        math = false,
        frontmatter = false,
        typographer = false,
        sourcepos = false,
        heading_anchors = false,
        syntax_highlighting = false,
        syntax_theme = None,
        syntax_classed = false
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        html: bool,
        linkify: bool,
        math: bool,
        frontmatter: bool,
        typographer: bool,
        sourcepos: bool,
        heading_anchors: bool,
        syntax_highlighting: bool,
        syntax_theme: Option<&str>,
        syntax_classed: bool,
    ) -> PyResult<Self> {
        let built = build(
            html,
            linkify,
            math,
            frontmatter,
            typographer,
            sourcepos,
            heading_anchors,
            syntax_highlighting,
            syntax_theme,
            syntax_classed,
        )?;

        Ok(Self {
            inner: built.inner,
            plugins: built.plugins,
        })
    }

    fn render(&self, src: &str) -> String {
        self.inner.parse(src).render()
    }

    fn parse(&self, py: Python<'_>, src: &str) -> PyResult<Py<PyAst>> {
        Py::new(
            py,
            PyAst {
                root: RefCell::new(self.inner.parse(src)),
            },
        )
    }

    fn parse_frontmatter(&self, src: &str) -> Option<PyFrontMatter> {
        let ast = self.inner.parse(src);
        let root = ast.cast::<Root>()?;
        root.ext.get::<FrontMatter>().map(PyFrontMatter::from)
    }

    fn render_with_frontmatter(&self, src: &str) -> PyMarkdownOutput {
        let ast = self.inner.parse(src);
        let frontmatter = ast
            .cast::<Root>()
            .and_then(|root| root.ext.get::<FrontMatter>())
            .map(PyFrontMatter::from);
        let html = ast.render();

        PyMarkdownOutput { html, frontmatter }
    }

    fn syntax_theme_css(&self) -> Option<String> {
        #[cfg(feature = "syntect")]
        {
            markdown_it::plugins::extra::syntect::theme_css(&self.inner)
        }

        #[cfg(not(feature = "syntect"))]
        {
            None
        }
    }

    #[pyo3(name = "use", signature = (plugin, *args, **kwargs))]
    fn use_plugin(
        slf: Py<Self>,
        py: Python<'_>,
        plugin: &Bound<'_, PyAny>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<Self>> {
        if let Ok(name) = plugin.extract::<&str>() {
            // use(plugin: str, **kwargs)
            if !args.is_empty() {
                return Err(PyTypeError::new_err(
                    "string plugins do not accept positional arguments",
                ));
            }

            let md = &mut *slf.borrow_mut(py);
            plugin_registry::enable(&mut md.inner, &mut md.plugins, name, kwargs)?;
        } else if plugin.is_callable() {
            // use(plugin: Callable, *arg, **kwargs)
            let mut new_args = vec![slf.clone_ref(py).into_bound(py).into_any()];
            for arg in args {
                new_args.push(arg.clone().into_any());
            }
            let new_args_tuple = PyTuple::new(py, new_args)?;
            plugin.call(new_args_tuple, kwargs)?;
        } else {
            return Err(PyTypeError::new_err(
                "plugin must be a string or a callable",
            ));
        }

        // chained calls.
        // md.use(...).use(...).use(...).render()
        Ok(slf)
    }
}
