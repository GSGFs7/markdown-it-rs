use std::cell::RefCell;

use markdown_it::document::{Document, NodeId, NodeValue, Text};
use markdown_it::render::{RenderOptions, RendererRegistry};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyclass(name = "Ast", unsendable)]
pub(crate) struct PyAst {
    pub(crate) root: RefCell<Document>,
    pub(crate) renderers: RendererRegistry,
    pub(crate) options: RenderOptions,
}

#[pymethods]
impl PyAst {
    #[getter]
    fn root(slf: PyRef<'_, Self>, py: Python<'_>) -> PyResult<Py<PyNode>> {
        let id = slf.root.borrow().root();
        Py::new(
            py,
            PyNode {
                ast: slf.into(),
                id,
            },
        )
    }
}

#[pyclass(name = "Node", unsendable)]
pub(crate) struct PyNode {
    pub(crate) ast: Py<PyAst>,
    pub(crate) id: NodeId,
}

#[pymethods]
impl PyNode {
    #[getter]
    fn type_name(&self, py: Python<'_>) -> PyResult<String> {
        let ast = self.ast.borrow(py);
        let doc = ast.root.borrow();
        let node = doc.get_node(self.id).ok_or_else(stale)?;
        Ok(node.name().to_owned())
    }

    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<Py<PyNode>>> {
        let ast = self.ast.borrow(py);
        let doc = ast.root.borrow();
        let node = doc.get_node(self.id).ok_or_else(stale)?;
        node.children()
            .iter()
            .map(|&id| {
                Py::new(
                    py,
                    PyNode {
                        ast: self.ast.clone_ref(py),
                        id,
                    },
                )
            })
            .collect()
    }

    fn render(&self, py: Python<'_>) -> PyResult<String> {
        let ast = self.ast.borrow(py);
        let doc = ast.root.borrow();
        doc.get_node(self.id).ok_or_else(stale)?;
        Ok(ast
            .renderers
            .render_subtree(&doc, self.id, "html", &ast.options))
    }

    fn append_text(&self, py: Python<'_>, text: &str) -> PyResult<()> {
        self.append(
            py,
            Text {
                content: text.to_owned(),
            },
        )
    }

    fn append_html(&self, py: Python<'_>, html: &str) -> PyResult<()> {
        self.append(
            py,
            markdown_it::plugins::html::html_inline::HtmlInline {
                content: html.to_owned(),
            },
        )
    }

    fn clear_children(&self, py: Python<'_>) -> PyResult<()> {
        let ast = self.ast.borrow(py);
        let mut doc = ast.root.borrow_mut();
        doc.get_node(self.id).ok_or_else(stale)?;
        doc.replace_children(self.id, Vec::new());
        Ok(())
    }
}

impl PyNode {
    fn append<T: NodeValue>(&self, py: Python<'_>, value: T) -> PyResult<()> {
        let ast = self.ast.borrow(py);
        let mut doc = ast.root.borrow_mut();
        doc.get_node(self.id).ok_or_else(stale)?;
        let child = doc.create_node(value);
        doc.push_child(self.id, child);
        Ok(())
    }
}

fn stale() -> PyErr {
    PyRuntimeError::new_err("stale node handle")
}
