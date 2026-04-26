use markdown_it::MarkdownIt;
use markdown_it::parser::core::Root;
use markdown_it::plugins::extra::front_matter::{FrontMatter, FrontMatterKind};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

#[allow(clippy::too_many_arguments)]
fn build(
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
) -> PyResult<MarkdownIt> {
    // avoid warning when syntect feature is disabled
    #[cfg(not(feature = "syntect"))]
    let _ = (syntax_theme, syntax_classed);

    let mut md = MarkdownIt::new();

    if enable_frontmatter {
        markdown_it::plugins::extra::front_matter::add(&mut md);
    }
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::tables::add(&mut md);
    markdown_it::plugins::extra::strikethrough::add(&mut md);
    markdown_it::plugins::extra::beautify_links::add(&mut md);
    if enable_heading_anchors {
        markdown_it::plugins::extra::heading_anchors::add(
            &mut md,
            markdown_it::plugins::extra::heading_anchors::simple_slugify_fn,
        );
    }
    if enable_html {
        markdown_it::plugins::html::add(&mut md);
    }
    if enable_linkify {
        #[cfg(feature = "linkify")]
        {
            markdown_it::plugins::extra::linkify::add(&mut md);
        }

        #[cfg(not(feature = "linkify"))]
        {
            return Err(PyValueError::new_err(
                "linkify requires the linkify feature",
            ));
        }
    }
    if enable_math {
        markdown_it::plugins::extra::math::add(&mut md);
    }
    if enable_typographer {
        markdown_it::plugins::extra::typographer::add(&mut md);
        markdown_it::plugins::extra::smartquotes::add(&mut md);
    }
    if enable_sourcepos {
        markdown_it::plugins::sourcepos::add(&mut md);
    }
    if enable_syntax_highlighting {
        #[cfg(feature = "syntect")]
        {
            markdown_it::plugins::extra::syntect::add(&mut md);
            if let Some(theme) = syntax_theme {
                if !markdown_it::plugins::extra::syntect::available_themes()
                    .iter()
                    .any(|available_theme| available_theme == theme)
                {
                    return Err(PyValueError::new_err(format!(
                        "unknown syntect theme: {theme}"
                    )));
                }
                markdown_it::plugins::extra::syntect::set_theme(&mut md, theme);
            }
            if syntax_classed {
                markdown_it::plugins::extra::syntect::set_to_classed(&mut md);
            }
        }

        #[cfg(not(feature = "syntect"))]
        {
            return Err(PyValueError::new_err(
                "syntax highlighting requires the syntect feature",
            ));
        }
    }

    Ok(md)
}

#[pyclass(name = "MarkdownIt")]
struct PyMarkdownIt {
    inner: MarkdownIt,
}

#[derive(Clone)]
#[pyclass(name = "FrontMatter", skip_from_py_object)]
struct PyFrontMatter {
    #[pyo3(get)]
    kind: String,
    #[pyo3(get)]
    raw: String,
    #[pyo3(get)]
    start_line: usize,
    #[pyo3(get)]
    end_line: usize,
}

#[pyclass(name = "MarkdownOutput")]
struct PyMarkdownOutput {
    #[pyo3(get)]
    html: String,
    #[pyo3(get)]
    frontmatter: Option<PyFrontMatter>,
}

impl From<&FrontMatter> for PyFrontMatter {
    fn from(front_matter: &FrontMatter) -> Self {
        let kind = match front_matter.kind {
            FrontMatterKind::Yaml => "yaml",
            FrontMatterKind::Toml => "toml",
        };

        Self {
            kind: kind.to_owned(),
            raw: front_matter.raw.clone(),
            start_line: front_matter.start_line,
            end_line: front_matter.end_line,
        }
    }
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
        Ok(Self {
            inner: build(
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
            )?,
        })
    }

    fn render(&self, src: &str) -> String {
        self.inner.parse(src).render()
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
}

#[pyfunction]
fn available_syntax_themes() -> Vec<String> {
    #[cfg(feature = "syntect")]
    {
        markdown_it::plugins::extra::syntect::available_themes()
    }

    #[cfg(not(feature = "syntect"))]
    {
        Vec::new()
    }
}

#[pymodule]
fn _markdown_it_rs_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMarkdownIt>()?;
    m.add_class::<PyFrontMatter>()?;
    m.add_class::<PyMarkdownOutput>()?;
    m.add_function(wrap_pyfunction!(available_syntax_themes, m)?)?;
    Ok(())
}
