use markdown_it::MarkdownIt;
use pyo3::prelude::*;

fn build(enable_html: bool, enable_linkify: bool) -> MarkdownIt {
    let mut md = MarkdownIt::new();
    markdown_it::plugins::cmark::add(&mut md);
    markdown_it::plugins::extra::tables::add(&mut md);
    markdown_it::plugins::extra::strikethrough::add(&mut md);
    markdown_it::plugins::extra::beautify_links::add(&mut md);

    if enable_html {
        markdown_it::plugins::html::add(&mut md);
    }
    if enable_linkify {
        markdown_it::plugins::extra::linkify::add(&mut md);
    }

    md
}

#[pyclass(name = "MarkdownIt")]
struct PyMarkdownIt {
    inner: MarkdownIt,
}

#[pymethods]
impl PyMarkdownIt {
    #[new]
    #[pyo3(signature= (
        *,
        html = false,
        linkify = false
    ))]
    fn new(html: bool, linkify: bool) -> Self {
        Self {
            inner: build(html, linkify),
        }
    }

    fn render(&self, src: &str) -> String {
        self.inner.parse(src).render()
    }
}

#[pymodule]
fn _markdown_it_rs_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMarkdownIt>()?;
    Ok(())
}
