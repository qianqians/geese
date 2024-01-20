use std::env;

use serde::{Deserialize, Serialize};
use tracing::{info, error};
use uuid::Uuid;
use consulrs::api::check::common::AgentServiceCheckBuilder;
use consulrs::api::service::requests::RegisterServiceRequest;

use health::HealthHandle;
use consul::ConsulImpl;
use log;
use config::{load_data_from_file, load_cfg_from_data};
use local_ip::get_local_ip;

use dbproxy::DBProxyServer;

#[derive(Deserialize, Serialize, Debug)]
struct DBProxyCfg {
    namespace: String,
    consul_url: String,
    health_port: u16,
    jaeger_url: Option<String>,
    redis_url: String,
    mongo_url: String,
    log_level: String,
    log_file: String,
    log_dir: String
}

#[tokio::main]
async fn main() {
    info!("dbproxy start!");

    let _name = format!("dbproxy_{}", Uuid::new_v4());

    let args: Vec<String> = env::args().collect();
    let cfg_file = &args[1];
    let cfg_data = match load_data_from_file(cfg_file.to_string()) {
        Err(e) => {
            println!("DBProxy load_data_from_file faild {}, {}!", cfg_file, e);
            return;
        },
        Ok(_cfg_data) => _cfg_data
    };
    let cfg = match load_cfg_from_data::<DBProxyCfg>(&cfg_data) {
        Err(e) => {
            println!("DBProxy load_cfg_from_data faild {}, {}!", cfg_data, e);
            return;
        },
        Ok(_cfg) => _cfg
    };

    let (_, _guard) = log::init(cfg.log_level, cfg.log_dir, cfg.log_file, cfg.jaeger_url, Some(_name.clone()));

    let health_port = cfg.health_port;
    let health_host = format!("0.0.0.0:{}", health_port);
    let health_handle = HealthHandle::new(health_host.clone());

    let mut server = match DBProxyServer::new(_name.clone(), cfg.redis_url, cfg.mongo_url, health_handle.clone()).await {
        Err(e) => {
            error!("DBProxy DBProxyServer new faild {}!", e);
            return;
        },
        Ok(_s) => _s
    };

    let _local_ip = get_local_ip();
    let _health_host = format!("http://{_local_ip}:{health_port}/health");
    let mut consul_impl = ConsulImpl::new(cfg.consul_url);
    consul_impl.register("dbproxy".to_string(), Some(
        RegisterServiceRequest::builder()
            .name("dbproxy")
            .id(_name)
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

    let _ = tokio::spawn(HealthHandle::start_health_service(health_host.clone(), health_handle.clone()));
    
    server.run().await;
    server.join().await;

    info!("dbproxy exit!");
}
