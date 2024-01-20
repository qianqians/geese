use pyo3::prelude::*;
use client::{ClientContext, ClientPump};

#[pymodule]
pub fn pyclient(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<ClientContext>()?;
    m.add_class::<ClientPump>()?;
    Ok(())
}