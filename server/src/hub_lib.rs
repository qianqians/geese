use pyo3::prelude::*;
use hub::{HubContext, HubConnMsgPump, HubDBMsgPump};

#[pymodule]
pub fn pyhub(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<HubContext>()?;
    m.add_class::<HubConnMsgPump>()?;
    m.add_class::<HubDBMsgPump>()?;
    physics::py::add_to_module(m)?;
    aoi::py::add_to_module(m)?;
    Ok(())
}