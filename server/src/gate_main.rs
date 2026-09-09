use std::env;
use std::sync::Arc;

use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use consulrs::api::check::common::AgentServiceCheckBuilder;
use consulrs::api::service::requests::RegisterServiceRequest;
use tracing::{trace, info, error};

use health::HealthHandle;
use consul::ConsulImpl;
use config::{load_data_from_file, load_cfg_from_data};

use gate::{WSSCfg, GateServer};

#[derive(Deserialize, Serialize, Debug)]
struct GateCfg {
    name: String,
    consul_url: String,
    health_port: u16,
    advertise_ip: String,
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

    let args: Vec<String> = env::args().collect();
    let cfg_file = match args.get(1) {
        Some(path) => path,
        None => {
            eprintln!("Usage: {} <config_file>", args.first().map(|s| s.as_str()).unwrap_or("gate"));
            std::process::exit(1);
        }
    };
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
    let _name = format!("gate_{}", cfg.name);

    let (_, _guard) = log::init(cfg.log_level, cfg.log_dir, cfg.log_file, cfg.jaeger_url, Some(_name.clone()));

    info!("gate log init!");

    let health_port = cfg.health_port;
    let health_host = format!("0.0.0.0:{}", health_port);
    let health_handle = HealthHandle::new(health_host.clone());

    let host = format!("0.0.0.0:{}", cfg.service_port);
    let client_tcp_host = cfg.client_tcp_port.map(|port| format!("0.0.0.0:{}", port));
    let client_ws_host = cfg.client_ws_port.map(|port| format!("0.0.0.0:{}", port));

    let _advertise_ip = cfg.advertise_ip.clone();
    let _health_host = format!("http://{_advertise_ip}:{health_port}/health");

    // 1. 先绑定健康检查端口，失败直接退出，避免“已注册但端点不可用”的竞态。
    let listener = match HealthHandle::bind(health_host.clone()).await {
        Ok(l) => l,
        Err(e) => {
            error!("Gate health bind failed {}!", e);
            return;
        }
    };

    let mut consul_impl = match ConsulImpl::new(cfg.consul_url) {
        Err(e) => {
            error!("Gate ConsulImpl new faild {}!", e);
            return;
        },
        Ok(c) => c
    };
    consul_impl.register("gate".to_string(), Some(
        RegisterServiceRequest::builder()
            .name("gate")
            .id(_name.clone())
            .address(_advertise_ip)
            .port(cfg.service_port)
            .check(AgentServiceCheckBuilder::default()
                .name("health_check")
                .interval("10s")
                .timeout("3s")
                .deregister_critical_service_after("20s")
                .http(_health_host)
                .status("passing")
                .build()
                .unwrap()
            ),
        ),
    ).await;

    trace!("server new consul_impl!");
    let _consul_impl_arc = Arc::new(Mutex::new(consul_impl));
    let _consul_impl_for_shutdown = _consul_impl_arc.clone();
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

    // 2. 用已绑定的 listener 提供健康检查服务。
    let health_service = tokio::spawn({
        let health_handle = health_handle.clone();
        async move {
            if let Err(e) = HealthHandle::serve(listener, health_handle).await {
                error!("health service error: {}", e);
            }
        }
    });

    trace!("server start run!");
    server.run().await;
    server.join().await;

    // 3. 退出前主动注销，避免 Consul 中的幽灵服务。
    {
        let mut _consul = _consul_impl_for_shutdown.lock().await;
        _consul.deregister(_name.clone()).await;
    }
    health_service.abort();

    info!("gate exit!");
}
