use std::env;
use local_ip_address::local_ip;

pub fn get_local_ip() -> String {
    match env::var("K8S_POD_IP") {
        Ok(ip) => ip,
        Err(_) => local_ip().unwrap().to_string()
    }
}