use pyo3::prelude::*;
use client::{ClientContext, ClientPump};

#[pymodule]
pub fn pyclient(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ClientContext>()?;
    m.add_class::<ClientPump>()?;
    Ok(())
}