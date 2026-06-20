use std::collections::{HashMap, VecDeque};

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;

pub(crate) struct PostProcessor {
    pub(crate) name: String,
    pub(crate) callback: Py<PyAny>,
    pub(crate) after: Vec<String>,
    pub(crate) enabled: bool,
}

pub(crate) struct PostProcessorManager {
    postprocessors: Vec<PostProcessor>,
    sorted_processors: Vec<usize>,
}

impl PostProcessorManager {
    pub(crate) fn new() -> Self {
        PostProcessorManager {
            postprocessors: Vec::new(),
            sorted_processors: Vec::new(),
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

        // build graph (in-degree)
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); self.postprocessors.len()];
        let mut in_degree: Vec<usize> = vec![0; self.postprocessors.len()];
        for &idx in &active {
            let p = &self.postprocessors[idx];
            for after_name in &p.after {
                let Some(&after_idx) = names.get(after_name.as_str()) else {
                    return Err(PyValueError::new_err(format!(
                        "unknown postprocessor dependency: {} after {}",
                        p.name, after_name
                    )));
                };

                deps[after_idx].push(idx);
                in_degree[idx] += 1;
            }
        }

        // topo sort (kahn)
        // a queue store 0 in-degree node
        let mut queue: VecDeque<usize> = active
            .iter()
            .copied()
            // put all 0 in-degree to queue
            .filter(|&idx| in_degree[idx] == 0)
            .collect();
        let mut result: Vec<usize> = Vec::with_capacity(active.len());
        while let Some(idx) = queue.pop_front() {
            // push 0 in-degree to result
            result.push(idx);

            // reduce in-degree
            for &dep_idx in &deps[idx] {
                in_degree[dep_idx] -= 1;
                // push to queue
                if in_degree[dep_idx] == 0 {
                    queue.push_back(dep_idx);
                }
            }
        }

        // cycle detection
        if result.len() != active.len() {
            let remaining = active
                .iter()
                .filter(|idx| in_degree[**idx] > 0)
                .map(|idx| self.postprocessors[*idx].name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(PyValueError::new_err(format!(
                "cyclic postprocessor dependency involving: {remaining}"
            )));
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
        self.postprocessors.push(PostProcessor {
            name: name.to_owned(),
            callback,
            after,
            enabled: true,
        });

        // sort all postprocessors
        if let Err(err) = self.sort() {
            self.postprocessors.pop();
            return Err(err);
        }

        Ok(())
    }

    pub(crate) fn run(&self, py: Python<'_>, mut html: String) -> PyResult<String> {
        for &idx in &self.sorted_processors {
            let p = &self.postprocessors[idx];
            html = p.callback.call1(py, (html,))?.extract::<String>(py)?;
        }
        Ok(html)
    }
}
