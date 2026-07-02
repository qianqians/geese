use tracing::{trace, info, warn, error};
use serde::{Deserialize, Serialize};

use consulrs::{catalog, service};
use consulrs::client::{ConsulClient, ConsulClientSettingsBuilder};
use consulrs::api::service::requests::RegisterServiceRequestBuilder;

pub struct ConsulImpl {
    client: ConsulClient
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ServiceInfo {
    pub id: String,
    pub name: String,
    pub addr: String,
    pub port: u16
}

impl ConsulImpl  {
    pub fn new(consul_url: String) -> Result<ConsulImpl, Box<dyn std::error::Error>> {
        let setting = ConsulClientSettingsBuilder::default()
            .address(consul_url)
            .build()?;
        let _client = ConsulClient::new(setting)?;
        Ok(ConsulImpl { client: _client })
    }

    pub async fn register(&mut self, name: String, opts: Option<&mut RegisterServiceRequestBuilder>) {
        match service::register(&self.client, &name, opts).await {
            Err(e) => {
                error!("consul register err:{}!", e);
            },
            Ok(_) => {
                info!("consul register success!");
            }
        }
    }

    pub async fn services(&mut self, name: String) -> Option<Vec<ServiceInfo>> {
        match catalog::nodes_with_service(&self.client, name.clone().as_str(), None).await {
            Err(e) => {
                error!("consul services err:{}!", e);
                None
            },
            Ok(rsp) => {
                info!("consul services success!");
                let mut infos: Vec<ServiceInfo> = Vec::new();
                if rsp.response.len() > 0 {
                    for service in rsp.response.iter() {
                        trace!("service:{:?}", service);
                        let service_id = match service.service_id.clone() {
                            Some(id) => id,
                            None => {
                                warn!("Consul service entry missing service_id, skipping");
                                continue;
                            }
                        };
                        let service_name = match service.service_name.clone() {
                            Some(name) => name,
                            None => {
                                warn!("Consul service entry missing service_name, skipping");
                                continue;
                            }
                        };
                        let service_address = match service.service_address.clone() {
                            Some(addr) => addr,
                            None => {
                                warn!("Consul service entry missing service_address, skipping");
                                continue;
                            }
                        };
                        let service_port = match service.service_port {
                            Some(port) => port as u16,
                            None => {
                                warn!("Consul service entry missing service_port, skipping");
                                continue;
                            }
                        };
                        let _service_info = ServiceInfo{
                            id: service_id,
                            name: service_name,
                            addr: service_address,
                            port: service_port,
                        };
                        infos.push(_service_info);
                    }
                }
                Some(infos)
            }
        }
    }
}