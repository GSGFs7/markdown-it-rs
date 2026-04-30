use pyo3::PyAny;
use pyo3::prelude::*;

pub(crate) struct PyCoreRule {
    pub(crate) name: String,
    pub(crate) callback: Py<PyAny>,
}

impl PyCoreRule {}
