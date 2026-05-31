use std::collections::HashMap;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

pub(crate) struct PostProcessor {
    pub(crate) name: String,
    pub(crate) callback: Py<PyAny>,
    pub(crate) after: Vec<String>,
    pub(crate) _order: usize, // registration order
    pub(crate) enabled: bool,
}

pub(crate) struct PostProcessorManager {
    postprocessors: Vec<PostProcessor>,
    sorted_processors: Vec<usize>,
    next_postprocessor_order: usize,
}

impl PostProcessorManager {
    pub(crate) fn new() -> Self {
        PostProcessorManager {
            postprocessors: Vec::new(),
            sorted_processors: Vec::new(),
            next_postprocessor_order: 1,
        }
    }

    /// try extract str or list[str]
    fn parse_after(after: Option<&Bound<'_, PyAny>>) -> PyResult<Vec<String>> {
        let Some(after) = after else {
            return Ok(vec![]);
        };

        if let Ok(s) = after.extract::<String>() {
            return Ok(vec![s]);
        }

        if let Ok(list) = after.extract::<Vec<String>>() {
            return Ok(list);
        }

        Err(PyTypeError::new_err(
            "after must be a string or a list of strings",
        ))
    }

    // TODO: optimize here. O(N^2)
    fn sort(&mut self) -> PyResult<()> {
        // collect all active plugins
        let active: Vec<usize> = self
            .postprocessors
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| p.enabled.then_some(idx))
            .collect();

        // mapping name & deps
        let mut names: HashMap<&str, usize> = HashMap::new();
        for &idx in &active {
            let p = &self.postprocessors[idx];
            names.insert(p.name.as_str(), idx);
        }

        // add to deps array
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); self.postprocessors.len()];
        for &idx in &active {
            let p = &self.postprocessors[idx];
            for after_name in &p.after {
                let Some(&after_idx) = names.get(after_name.as_str()) else {
                    return Err(PyValueError::new_err(format!(
                        "unknown postprocessor dependency: {} after {}",
                        p.name, after_name
                    )));
                };

                deps[idx].push(after_idx);
            }
        }

        // topo sort
        let mut inserted: Vec<bool> = vec![false; self.postprocessors.len()];
        let mut result: Vec<usize> = Vec::with_capacity(active.len());
        while result.len() < active.len() {
            let mut progressed = false;
            for &idx in &active {
                if inserted[idx] {
                    continue;
                }

                if deps[idx].iter().all(|dep_idx| inserted[*dep_idx]) {
                    result.push(idx);
                    inserted[idx] = true;
                    progressed = true;
                }
            }

            if !progressed {
                let remaining = active
                    .iter()
                    .filter(|idx| !inserted[**idx])
                    .map(|idx| self.postprocessors[*idx].name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(PyValueError::new_err(format!(
                    "cyclic postprocessor dependency involving: {remaining}"
                )));
            }
        }

        self.sorted_processors = result;
        Ok(())
    }
    pub(crate) fn add(
        &mut self,
        py: Python<'_>,
        name: &str,
        callback: Py<PyAny>,
        after: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        // is callable?
        if !callback.bind(py).is_callable() {
            return Err(PyTypeError::new_err("postprocessor must be a callable"));
        }

        // exist same name?
        if self.postprocessors.iter().any(|p| p.name == name) {
            return Err(PyValueError::new_err(format!(
                "postprocessor already exists: {name}"
            )));
        }

        // get the deps
        let after = Self::parse_after(after)?;

        // push
        let order = self.next_postprocessor_order;
        self.postprocessors.push(PostProcessor {
            name: name.to_owned(),
            callback,
            after,
            _order: order,
            enabled: true,
        });

        // sort all postprocessors
        if let Err(err) = self.sort() {
            self.postprocessors.pop();
            return Err(err);
        }

        self.next_postprocessor_order += 1;

        Ok(())
    }

    pub(crate) fn run(&self, py: Python<'_>, mut html: String) -> PyResult<String> {
        for &idx in &self.sorted_processors {
            let postprocessor = &self.postprocessors[idx];
            html = postprocessor
                .callback
                .call1(py, (html,))?
                .extract::<String>(py)?;
        }
        Ok(html)
    }
}
