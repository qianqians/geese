use std::env;

use tracing::{info, error};
use consulrs::api::check::common::AgentServiceCheckBuilder;
use consulrs::api::service::requests::RegisterServiceRequest;

use health::HealthHandle;
use consul::ConsulImpl;
use config::{load_data_from_file, load_cfg_from_data};

use dbproxy::{DBProxyServer, DBProxyCfg};

#[tokio::main]
async fn main() {
    info!("dbproxy start!");

    let args: Vec<String> = env::args().collect();
    let cfg_file = match args.get(1) {
        Some(path) => path,
        None => {
            eprintln!("Usage: {} <config_file>", args.first().map(|s| s.as_str()).unwrap_or("dbproxy"));
            std::process::exit(1);
        }
    };
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
    let _name = format!("dbproxy_{}", cfg.name);

    let (_, _guard) = log::init(cfg.log_level, cfg.log_dir, cfg.log_file, cfg.jaeger_url, Some(_name.clone()));

    let health_port = cfg.health_port;
    let health_host = format!("0.0.0.0:{}", health_port);
    let health_handle = HealthHandle::new(health_host.clone());

    // 1. 先绑定健康检查端口，失败直接退出。
    let listener = match HealthHandle::bind(health_host.clone()).await {
        Ok(l) => l,
        Err(e) => {
            error!("DBProxy health bind failed {}!", e);
            return;
        }
    };

    let mut server = match DBProxyServer::new(_name.clone(), cfg.redis_url, cfg.mongo_url, cfg.index, cfg.guid, health_handle.clone()).await {
        Err(e) => {
            error!("DBProxy DBProxyServer new faild {}!", e);
            return;
        },
        Ok(_s) => _s
    };

    let _advertise_ip = cfg.advertise_ip.clone();
    let _health_host = format!("http://{_advertise_ip}:{health_port}/health");
    let mut consul_impl = match ConsulImpl::new(cfg.consul_url) {
        Err(e) => {
            error!("DBProxy ConsulImpl new faild {}!", e);
            return;
        },
        Ok(c) => c
    };
    consul_impl.register("dbproxy".to_string(), Some(
        RegisterServiceRequest::builder()
            .name("dbproxy")
            .id(_name.clone())
            .address(_advertise_ip)
            .port(cfg.service_port)
            .check(AgentServiceCheckBuilder::default()
                .name("health_check")
                .interval("10s")
                .timeout("15s")
                .deregister_critical_service_after("30s")
                .http(_health_host)
                .status("passing")
                .build()
                .unwrap()
            ),
        ),
    ).await;

    // 2. 用已绑定的 listener 提供健康检查服务。
    let health_service = tokio::spawn({
        let health_handle = health_handle.clone();
        async move {
            if let Err(e) = HealthHandle::serve(listener, health_handle).await {
                error!("health service error: {}", e);
            }
        }
    });
    
    server.run().await;
    server.join().await;

    // 3. 退出前主动注销。
    consul_impl.deregister(_name.clone()).await;
    health_service.abort();

    info!("dbproxy exit!");
}
