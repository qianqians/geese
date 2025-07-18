use std::sync::Arc;
use std::collections::BTreeMap;

use tokio::task::JoinHandle;
use tokio::sync::Mutex;
use uuid::Uuid;
use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;
use redis::{Client, Commands, Connection, RedisError};
use async_trait::async_trait;
use tracing::{trace, error};

use net::NetWriter;
use proto::common::RedisMsg;
use close_handle::CloseHandle;

use crate::redis_mq_channel::{RedisMQReader, RedisMQWriter};

pub fn create_lock_key(server_name1:String, server_name2:String) -> String {
    if server_name1 > server_name2 {
        return format!("{}:{}", server_name1, server_name2);
    }
    else {
        return format!("{}:{}", server_name2, server_name1);
    }
}

pub fn create_channel_key(name:String) -> String {
    return format!("{}:channel", name);
}

pub fn create_host_cache_key(name:String) -> String {
    return format!("{}:host", name);
}

pub struct RedisService{
    lname: String,
    client: Arc<Mutex<Client>>,
    conn: Arc<Mutex<Connection>>,
    join: Option<JoinHandle<()>>,
    rds: Arc<Mutex<BTreeMap<String, (Arc<Mutex<RedisMQReader>>, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>)>>>
}

#[async_trait]
pub trait RedisMQListenCallback {
    async fn redis_mq_cb(&mut self, rd: Arc<Mutex<RedisMQReader>>, wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>);
}

impl RedisService {
    pub async fn new(host:String, lname:String) -> Result<RedisService, Box<dyn std::error::Error>> {
        let client = redis::Client::open(host)?;
        let conn = Arc::new(Mutex::new(client.get_connection()?));

        let rds: Arc<Mutex<BTreeMap<String, (Arc<Mutex<RedisMQReader>>, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>)>>> = Arc::new(Mutex::new(BTreeMap::new()));

        Ok(RedisService {
            lname: lname,
            client: Arc::new(Mutex::new(client)),
            conn: conn,
            join: None,
            rds: rds,
        })
    }

    pub async fn listen(
        host:String, 
        lname:String,
        close: Arc<Mutex<CloseHandle>>,
        f:Arc<Mutex<Box<dyn RedisMQListenCallback + Send + 'static>>>) -> Result<RedisService, Box<dyn std::error::Error>> 
    {
        trace!("redis mq listen:{}", lname);

        let client = redis::Client::open(host)?;
        let conn = Arc::new(Mutex::new(client.get_connection()?));

        let rds: Arc<Mutex<BTreeMap<String, (Arc<Mutex<RedisMQReader>>, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>)>>> = Arc::new(Mutex::new(BTreeMap::new()));

        let service_lname = lname.clone();
        let service_conn = conn.clone();
        let service_rds = rds.clone();

        let _f_clone = f.clone();

        let _join = tokio::spawn(async move {
            loop {
                {
                    let _c_ref = close.as_ref().lock().await;
                    if _c_ref.is_closed() {
                        break;
                    }
                }

                let vec_data: Vec<Vec<u8>>;
                {
                    let pop_lname = lname.clone();
                    let mut _c = conn.as_ref().lock().await;
                    vec_data = match _c.brpop(pop_lname, 1.0) {
                        Err(e) => {
                            error!("RedisService brpop loop err:{}!", e);
                            continue;
                        },
                        Ok(_d) => _d
                    };
                    if vec_data.len() <= 0 {
                        continue;
                    }
                }

                trace!("RedisService brpop vec_data:{:?}", vec_data);
                let data = vec_data[1].clone();
                let mut t = TBufferChannel::with_capacity(data.len(), 0);
                let _ = t.set_readable_bytes(&data);
                let mut i_prot = TCompactInputProtocol::new(t);
                let ev_data = match RedisMsg::read_from_in_protocol(&mut i_prot) {
                    Err(e) => {
                        error!("RedisMsg read_from_in_protocol loop err:{}!", e);
                        continue;
                    },
                    Ok(_d) => _d
                };

                trace!("RedisService brpop lname:{:?}", lname);

                let rname = ev_data.server_name.unwrap();
                let rds_rname = rname.clone();
                let mut rds_ref = rds.as_ref().lock().await;
                if let Some((arc_rmqrd, _)) = rds_ref.get(&rname) {
                    let mut rmqrd = arc_rmqrd.as_ref().lock().await;
                    rmqrd.cb(ev_data.msg.unwrap()).await;
                }
                else {
                    let rd = Arc::new(Mutex::new(RedisMQReader::new()));
                    let wr_lname = lname.clone();
                    let wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = 
                        Arc::new(Mutex::new(Box::new(RedisMQWriter::new(conn.clone(), wr_lname, rname))));
                        
                    let mut f_handle = _f_clone.as_ref().lock().await;
                    f_handle.redis_mq_cb(rd.clone(), wr.clone()).await;

                    let rd_cb_data = rd.clone();
                    let mut rd_ref = rd_cb_data.as_ref().lock().await;
                    rd_ref.cb(ev_data.msg.unwrap()).await;
                    rds_ref.insert(rds_rname, (rd, wr));
                }         
            }
        });

        Ok(RedisService {
            lname: service_lname,
            client: Arc::new(Mutex::new(client)),
            conn: service_conn,
            join: Some(_join),
            rds: service_rds
        })
    }

    pub async fn join(self) {
        if let Some(_join) = self.join {
            let _ = _join.await;
        }
    }

    pub async fn connect(&mut self, rname: String) -> Result<(Arc<Mutex<RedisMQReader>>, Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>), Box<dyn std::error::Error>> {
        trace!("redis mq connect:{}", rname);

        let mut rds_ref = self.rds.as_ref().lock().await;
        if let Some((arc_rmqrd, arc_rmqwr)) = rds_ref.get(&rname) {
            Ok((arc_rmqrd.clone(), arc_rmqwr.clone()))
        }
        else {
            let rd = Arc::new(Mutex::new(RedisMQReader::new()));
            let wr: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>> = 
                Arc::new(Mutex::new(Box::new(RedisMQWriter::new(self.conn.clone(), self.lname.clone(), rname.clone()))));
    
            rds_ref.insert(rname, (rd.clone(), wr.clone()));
    
            Ok((rd, wr))
        }
    }

    pub async fn acquire_lock(&mut self, lock_key: String, timeout: usize) -> String {
        let value = Uuid::new_v4().to_string();
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            match conn_ref.set_nx(lock_key.clone(), value.clone()) {
                Ok(ret) => {
                    if ret {
                        break;
                    }
                },
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            } 
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            let ret: Result<(), RedisError> = conn_ref.expire(lock_key.clone(), timeout as i64);
            match ret {
                Ok(_) => {
                    break;
                },
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
        return value;
    }

    pub async fn release_lock(&mut self, lock_key: String, value: String) {
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            let v: Result<String, RedisError> = conn_ref.get(lock_key.clone());
            match v {
                Ok(_old_value) =>  {
                    if _old_value == value {
                        let ret: Result<(), RedisError> = conn_ref.del(lock_key.clone());
                        match ret {
                            Ok(_) => {
                                break;
                            },
                            Err(_) => {
                                if let Ok(conn) = self.client.blocking_lock().get_connection() {
                                    self.conn = Arc::new(Mutex::new(conn));
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    pub async fn set(&mut self, key: String, value: String, timeout:usize) {
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            let ret: Result<(), RedisError> = conn_ref.set_ex(key.clone(), value.clone(), timeout as u64);
            match ret {
                Ok(_) => {
                    break;
                },
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    pub async fn get(&mut self, key: String) -> String {
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            match conn_ref.get(key.clone()) {
                Ok(v) => return v,
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
    }

    pub async fn expire(&mut self, key: String, timeout:usize) -> redis::RedisResult<()> {
        loop {
            let _c = self.conn.clone();
            let mut conn_ref = _c.as_ref().lock().await;
            let ret: Result<(), RedisError> = conn_ref.expire(key.clone(), timeout as i64);
            match ret {
                Ok(_) => {
                    break;
                },
                Err(_) => {
                    if let Ok(conn) = self.client.blocking_lock().get_connection() {
                        self.conn = Arc::new(Mutex::new(conn));
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
        }
        Ok(())
    }
}