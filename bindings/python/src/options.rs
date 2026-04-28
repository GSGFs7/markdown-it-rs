use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyDictMethods};

pub(crate) fn require_no_options(
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    if options.is_some_and(|options| !options.is_empty()) {
        return Err(PyValueError::new_err(format!(
            "{plugin} does not accept options"
        )));
    }
    Ok(())
}

pub(crate) fn validate_options(
    plugin: &str,
    options: Option<&Bound<'_, PyDict>>,
    allowed: &[&str],
) -> PyResult<()> {
    let Some(options) = options else {
        return Ok(());
    };

    for (key, _) in options {
        let key = key
            .extract::<&str>()
            .map_err(|_| PyTypeError::new_err(format!("{plugin} option names must be strings")))?;
        if !allowed.contains(&key) {
            return Err(PyValueError::new_err(format!(
                "unknown option for {plugin}: {key}"
            )));
        }
    }

    Ok(())
}

#[cfg(feature = "syntect")]
pub(crate) fn get_bool(options: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<bool>> {
    let Some(options) = options else {
        return Ok(None);
    };
    options
        .get_item(key)?
        .map(|value| value.extract::<bool>())
        .transpose()
}

#[cfg(feature = "syntect")]
pub(crate) fn get_string(
    options: Option<&Bound<'_, PyDict>>,
    key: &str,
) -> PyResult<Option<String>> {
    let Some(options) = options else {
        return Ok(None);
    };
    options
        .get_item(key)?
        .map(|value| value.extract::<String>())
        .transpose()
}

pub(crate) fn get_usize(options: Option<&Bound<'_, PyDict>>, key: &str) -> PyResult<Option<usize>> {
    let Some(options) = options else {
        return Ok(None);
    };
    options
        .get_item(key)?
        .map(|value| value.extract::<usize>())
        .transpose()
}
