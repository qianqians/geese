use tracing::{trace, info, error};
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
    pub fn new(consul_url: String) -> ConsulImpl {
        let setting = ConsulClientSettingsBuilder::default()
            .address(consul_url)
            .build()
            .unwrap();
        let _client = ConsulClient::new(setting).unwrap();
        ConsulImpl { client: _client }
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
                        let _service_info = ServiceInfo{
                            id: service.service_id.clone().unwrap(),
                            name: service.service_name.clone().unwrap(),
                            addr: service.service_address.clone().unwrap(),
                            port: service.service_port.unwrap() as u16,
                        };
                        infos.push(_service_info);
                    }
                }
                Some(infos)
            }
        }
    }
}