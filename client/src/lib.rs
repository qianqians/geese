use pyo3::prelude::*;

#[pymodule]
pub fn pyclient(m: &Bound<'_, PyModule>) -> PyResult<()> {
    client::add_to_module(m)?;
    Ok(())
}