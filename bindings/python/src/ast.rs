use std::cell::RefCell;

use markdown_it::parser::inline::Text;
use markdown_it::{Node, NodeValue, Renderer};
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

#[pyclass(name = "Node", unsendable)]
pub(crate) struct PyNode {
    pub(crate) ast: Py<PyAst>,
    pub(crate) path: Vec<usize>,
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

    fn append_text(&self, py: Python<'_>, text: &str) -> PyResult<()> {
        self.with_node_mut(py, |node| {
            node.children.push(Node::new(Text {
                content: text.to_owned(),
            }));
        })
    }

    fn append_html(&self, py: Python<'_>, html: &str) -> PyResult<()> {
        self.with_node_mut(py, |node| {
            node.children.push(Node::new(RawHtml {
                html: html.to_owned(),
            }));
        })
    }

    fn clear_children(&self, py: Python<'_>) -> PyResult<()> {
        self.with_node_mut(py, |node| {
            node.children.clear();
        })
    }
}

impl PyNode {
    fn with_node_mut(&self, py: Python<'_>, f: impl FnOnce(&mut Node)) -> PyResult<()> {
        let ast = self.ast.borrow(py);
        let mut root = ast.root.borrow_mut();
        let node = node_at_path_mut(&mut root, &self.path)
            .ok_or_else(|| PyRuntimeError::new_err("stale node handle"))?;
        f(node);
        Ok(())
    }
}

// --- helper method ---

fn node_at_path<'a>(mut node: &'a Node, path: &[usize]) -> Option<&'a Node> {
    for &idx in path {
        node = node.children.get(idx)?
    }
    Some(node)
}

fn node_at_path_mut<'a>(mut node: &'a mut Node, path: &[usize]) -> Option<&'a mut Node> {
    for &idx in path {
        node = node.children.get_mut(idx)?
    }
    Some(node)
}

#[derive(Debug)]
struct RawHtml {
    html: String,
}

impl NodeValue for RawHtml {
    fn render(&self, _: &Node, fmt: &mut dyn Renderer) {
        fmt.text_raw(&self.html);
    }
}
