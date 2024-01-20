use std::fs;
use std::io;
use serde::{Deserialize, Serialize};

pub fn load_data_from_file(cfg_file: String) -> Result<String, io::Error> {
    let data = fs::read_to_string(cfg_file)?;
    Ok(data)
}

pub fn load_cfg_from_data<'a, C: Deserialize<'a> + Serialize>(data: &'a String) -> Result<C, io::Error> {
    let cfg: C = serde_json::from_str::<C>(&data)?;
    Ok(cfg)
}