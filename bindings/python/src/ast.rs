use std::cell::RefCell;

use markdown_it::{
    Document,
    DocumentRendererRegistry,
    EditBatch,
    NodeDraft,
    NodeId,
    RenderOptions,
    Text,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

#[pyclass(name = "Ast", unsendable)]
pub(crate) struct PyAst {
    pub(crate) root: RefCell<Document>,
    pub(crate) renderers: DocumentRendererRegistry,
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
            NodeDraft::new(Text {
                content: text.to_owned(),
            }),
        )
    }

    fn append_html(&self, py: Python<'_>, html: &str) -> PyResult<()> {
        self.append(
            py,
            NodeDraft::new(markdown_it::plugins::html::html_inline::HtmlInline {
                content: html.to_owned(),
            }),
        )
    }

    fn clear_children(&self, py: Python<'_>) -> PyResult<()> {
        let ast = self.ast.borrow(py);
        let mut doc = ast.root.borrow_mut();
        let node = doc.get_node(self.id).ok_or_else(stale)?;
        let mut edits = EditBatch::new();
        for &id in node.children() {
            edits.remove_node(id);
        }
        edits.commit(&mut doc);
        Ok(())
    }
}

impl PyNode {
    fn append(&self, py: Python<'_>, draft: NodeDraft) -> PyResult<()> {
        let ast = self.ast.borrow(py);
        let mut doc = ast.root.borrow_mut();
        doc.get_node(self.id).ok_or_else(stale)?;
        doc.append_child(self.id, draft);
        Ok(())
    }
}

fn stale() -> PyErr {
    PyRuntimeError::new_err("stale node handle")
}
