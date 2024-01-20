use pyo3::prelude::*;
use hub::{HubContext, HubConnMsgPump, HubDBMsgPump};

#[pymodule]
pub fn pyhub(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<HubContext>()?;
    m.add_class::<HubConnMsgPump>()?;
    m.add_class::<HubDBMsgPump>()?;
    Ok(())
}