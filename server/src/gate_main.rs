use std::env;
use std::sync::Arc;

use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use consulrs::api::check::common::AgentServiceCheckBuilder;
use consulrs::api::service::requests::RegisterServiceRequest;
use tracing::{trace, info, error};

use health::HealthHandle;
use consul::ConsulImpl;
use config::{load_data_from_file, load_cfg_from_data};
use local_ip::get_local_ip;

use gate::{WSSCfg, GateServer};

#[derive(Deserialize, Serialize, Debug)]
struct GateCfg {
    consul_url: String,
    health_port: u16,
    jaeger_url: Option<String>,
    redis_url: String,
    service_port: u16,
    client_tcp_port: Option<u16>,
    client_ws_port: Option<u16>,
    client_wss_cfg: Option<WSSCfg>,
    log_level: String,
    log_file: String,
    log_dir: String
}

#[tokio::main]
async fn main() {
    info!("gate start!");

    let _name = format!("gate_{}", Uuid::new_v4());

    let args: Vec<String> = env::args().collect();
    let cfg_file = &args[1];
    let cfg_data = match load_data_from_file(cfg_file.to_string()) {
        Err(e) => {
            println!("gate load_data_from_file faild {}, {}!", cfg_file, e);
            return;
        },
        Ok(_cfg_data) => _cfg_data
    };
    let cfg = match load_cfg_from_data::<GateCfg>(&cfg_data) {
        Err(e) => {
            println!("gate load_cfg_from_data faild {}, {}!", cfg_data, e);
            return;
        },
        Ok(_cfg) => _cfg
    };

    let (_, _guard) = log::init(cfg.log_level, cfg.log_dir, cfg.log_file, cfg.jaeger_url, Some(_name.clone()));

    info!("gate log init!");

    let health_port = cfg.health_port;
    let health_host = format!("0.0.0.0:{}", health_port);
    let health_handle = HealthHandle::new(health_host.clone());

    let host = format!("0.0.0.0:{}", cfg.service_port);
    let client_tcp_host = cfg.client_tcp_port.map(|port| format!("0.0.0.0:{}", port));
    let client_ws_host = cfg.client_ws_port.map(|port| format!("0.0.0.0:{}", port));

    let _local_ip = get_local_ip();
    let _health_host = format!("http://{_local_ip}:{health_port}/health");
    let mut consul_impl = ConsulImpl::new(cfg.consul_url);
    consul_impl.register("gate".to_string(), Some(
        RegisterServiceRequest::builder()
            .name("gate")
            .id(_name.clone())
            .address(_local_ip)
            .port(cfg.service_port)
            .check(AgentServiceCheckBuilder::default()
                .name("health_check")
                .interval("10s")
                .http(_health_host)
                .status("passing")
                .build()
                .unwrap()
            ),
        ),
    ).await;

    trace!("server new consul_impl!");
    let _consul_impl_arc = Arc::new(Mutex::new(consul_impl));
    let mut server = match GateServer::new(
        _name.clone(), 
        host, 
        cfg.redis_url,
        client_tcp_host, 
        client_ws_host, 
        cfg.client_wss_cfg, 
        _consul_impl_arc, 
        health_handle.clone()).await
    {
        Err(e) => {
            error!("Gate GateServer new faild {}!", e);
            return;
        },
        Ok(_s) => _s
    };
    trace!("server new server!");

    let health_service = tokio::spawn(HealthHandle::start_health_service(health_host.clone(), health_handle.clone()));

    trace!("server start run!");
    server.run().await;
    server.join().await;
    health_service.abort();

    info!("gate exit!");
}
