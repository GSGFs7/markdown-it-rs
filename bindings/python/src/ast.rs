use std::cell::RefCell;

use markdown_it::Node;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyclass(name = "Ast", unsendable)]
pub(crate) struct PyAst {
    pub(crate) root: RefCell<Node>,
}

#[pymethods]
impl PyAst {
    #[getter]
    fn root(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyNode>> {
        Py::new(
            py,
            PyNode {
                ast: slf.into(),
                path: Vec::new(),
            },
        )
    }
}

#[pyclass(name = "Node")]
pub(crate) struct PyNode {
    ast: Py<PyAst>,
    path: Vec<usize>,
}

#[pymethods]
impl PyNode {
    #[getter]
    fn type_name(&self, py: Python<'_>) -> PyResult<String> {
        let ast = self.ast.borrow(py);
        let root = ast.root.borrow();
        let node = node_at_path(&root, &self.path)
            .ok_or_else(|| PyRuntimeError::new_err("stale node handle"))?;
        Ok(node.name().to_owned())
    }

    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<Py<PyNode>>> {
        let ast = self.ast.borrow(py);
        let root = ast.root.borrow();
        let node = node_at_path(&root, &self.path)
            .ok_or_else(|| PyRuntimeError::new_err("stale node handle"))?;

        let mut result = Vec::with_capacity(node.children.len());
        for idx in 0..node.children.len() {
            let mut path = self.path.clone();
            path.push(idx);
            result.push(Py::new(
                py,
                PyNode {
                    ast: self.ast.clone_ref(py),
                    path,
                },
            )?);
        }

        Ok(result)
    }

    fn render(&self, py: Python<'_>) -> PyResult<String> {
        let ast = self.ast.borrow(py);
        let root = ast.root.borrow();
        let node = node_at_path(&root, &self.path)
            .ok_or_else(|| PyRuntimeError::new_err("stale node handle"))?;
        Ok(node.render())
    }
}

// --- helper method ---

fn node_at_path<'a>(mut node: &'a Node, path: &[usize]) -> Option<&'a Node> {
    for &idx in path {
        node = node.children.get(idx)?
    }
    Some(node)
}
