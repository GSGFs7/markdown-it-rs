pub(crate) mod builder;
pub(crate) mod markdown_it_py;
pub(crate) mod options;
pub(crate) mod plugin_registry;
pub(crate) mod plugin_state;
pub(crate) mod plugins;
pub(crate) mod types;

use pyo3::prelude::*;

use crate::markdown_it_py::PyMarkdownIt;
use crate::plugins::available_syntax_themes;
use crate::types::{PyFrontMatter, PyMarkdownOutput};

#[pymodule]
fn _markdown_it_rs_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMarkdownIt>()?;
    m.add_class::<PyFrontMatter>()?;
    m.add_class::<PyMarkdownOutput>()?;
    m.add_function(wrap_pyfunction!(available_syntax_themes, m)?)?;
    Ok(())
}
