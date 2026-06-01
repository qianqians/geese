use std::any::Any;
use std::cmp;
use std::sync::Weak;

use tokio::sync::Mutex;
use net::NetWriter;
use thrift::protocol::{TCompactOutputProtocol, TSerializable};
use thrift::transport::{TIoChannel, TBufferChannel};

use tracing::{trace, error};
use mongodb::bson::{doc, Document};

use proto::hub::{
    DbCallback, 
    AckGetGuid, 
    AckCreateObject, 
    AckUpdataObject, 
    AckFindAndModify, 
    AckRemoveObject, 
    AckGetObjectCount, 
    AckGetObjectInfo, 
    AckGetObjectInfoEnd
};

use mongo::MongoProxy;

pub enum DBEventType {
    EvGetGuid,
    EvCreateObject,
    EvUpdataObject,
    EvFindAndModify,
    EvRemoveObject,
    EvGetObjectInfo,
    EvGetObjectCount
}

pub struct DBEvGetGuid {
}

impl DBEvGetGuid {
    pub fn new() -> DBEvGetGuid {
        DBEvGetGuid {}
    }
}

pub struct DBEvCreateObject {
    pub object_info: Vec<u8>
}

impl DBEvCreateObject {
    pub fn new(_object_info:Vec<u8>) -> DBEvCreateObject {
        DBEvCreateObject {
            object_info: _object_info
        }
    }
}

pub struct DBEvUpdataObject {
    pub query_info: Vec<u8>,
    pub updata_info: Vec<u8>,
    pub upsert: bool
}

impl DBEvUpdataObject {
    pub fn new(_query_info: Vec<u8>, _updata_info: Vec<u8>, _upsert: bool) -> DBEvUpdataObject {
        DBEvUpdataObject {
            query_info: _query_info,
            updata_info: _updata_info,
            upsert: _upsert
        }
    }
}

pub struct DBEvFindAndModify {
    pub query_info: Vec<u8>, 
    pub updata_info: Vec<u8>, 
    pub _new: bool, 
    pub upsert: bool
}

impl DBEvFindAndModify {
    pub fn new(_query_info: Vec<u8>, _updata_info: Vec<u8>, _new_: bool, _upsert: bool) -> DBEvFindAndModify {
        DBEvFindAndModify {
            query_info: _query_info,
            updata_info: _updata_info,
            _new: _new_,
            upsert: _upsert
        }
    }
}

pub struct DBEvRemoveObject {
    pub query_info: Vec<u8>
}

impl DBEvRemoveObject {
    pub fn new(_query_info:Vec<u8>) -> DBEvRemoveObject {
        DBEvRemoveObject {
            query_info: _query_info
        }
    }
}

pub struct DBEvGetObjectInfo {
    pub query_info: Vec<u8>, 
    pub skip: u32, 
    pub limit: u32, 
    pub sort: String, 
    pub ascending: bool
}

impl DBEvGetObjectInfo {
    pub fn new(_query_info:Vec<u8>, _skip: i32, _limit: i32, _sort: String, _ascending: bool) -> DBEvGetObjectInfo {
        DBEvGetObjectInfo {
            query_info: _query_info,
            skip: _skip as u32,
            limit: _limit as u32,
            sort: _sort,
            ascending: _ascending
        }
    }
}

pub struct DBEvGetObjectCount {
    pub query_info: Vec<u8>
}

impl DBEvGetObjectCount {
    pub fn new(_query_info:Vec<u8>) -> DBEvGetObjectCount {
        DBEvGetObjectCount {
            query_info: _query_info
        }
    }
}

pub struct DBEvent {
    pub send_proxy: Weak<Mutex<Box<dyn NetWriter + Send + 'static>>>,
    pub ev_type: DBEventType,
    pub db: String,
    pub collection: String,
    pub callback_id: String,
    pub ev_data: Box<dyn Any>
}

unsafe impl Send for DBEvent {}

impl DBEvent {
    pub fn new(_send_proxy: Weak<Mutex<Box<dyn NetWriter + Send + 'static>>>, _ev_type: DBEventType, _db: String, _collection: String,  _callback_id: String, _ev_data: Box<dyn Any>) -> DBEvent {
        DBEvent {
            send_proxy: _send_proxy,
            ev_type: _ev_type,
            db: _db,
            collection: _collection,
            callback_id: _callback_id,
            ev_data: _ev_data
        }
    }

    async fn do_get_guid(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_get_guid");
        let guid = mongo_proxy.get_guid(self.db.to_string(), self.collection.to_string()).await;
        let cb = DbCallback::GetGuid(AckGetGuid::new(self.callback_id.to_string(), guid));
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        let tmp = rd.write_bytes();
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&tmp).await;
        }
        else {
            error!("do_get_guid send_proxy is destory!");
        }
    }

    async fn do_create_object(&mut self, mongo_proxy:&mut MongoProxy){
        trace!("begin do_create_object");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvCreateObject>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_create_object p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let result = mongo_proxy.save(self.db.to_string(), self.collection.to_string(), &ev_data.object_info).await;
        let cb = DbCallback::CreateObject(AckCreateObject::new(self.callback_id.to_string(), result));
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_create_object send_proxy is destory!");
        }
    }

    async fn do_updata_object(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_updata_object");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvUpdataObject>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_updata_object p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let result = mongo_proxy.update(self.db.to_string(), self.collection.to_string(), &ev_data.query_info, &ev_data.updata_info, ev_data.upsert).await;
        let cb = DbCallback::UpdataObject(AckUpdataObject::new(self.callback_id.to_string(), result));
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_updata_object send_proxy is destory!");
        }
    }

    async fn do_find_and_modify(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_find_and_modify");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvFindAndModify>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_find_and_modify p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let result = mongo_proxy.find_and_modify(self.db.to_string(), self.collection.to_string(), &ev_data.query_info, &ev_data.updata_info, ev_data._new, ev_data.upsert).await;
        let opt_doc = match result {
            Err(err) => {
                error!("do_find_and_modify find_and_modify err:{}!", err);
                return;
            },
            Ok(v) => v
        };
        let doc = match opt_doc {
            None => doc!{},
            Some(v) => v
        };
        let mut bin: Vec<u8> = Vec::new();
        let _ = doc.to_writer(&mut bin);
        let wsize = (bin.len() + 2047) / 1024 * 1024;
        let cb = DbCallback::FindAndModify(AckFindAndModify::new(self.callback_id.to_string(), bin));
        let t = TBufferChannel::with_capacity(0, wsize);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_find_and_modify send_proxy is destory!");
        }
    }

    async fn do_remove_object(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_remove_object");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvRemoveObject>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_remove_object p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let result = mongo_proxy.remove(self.db.to_string(), self.collection.to_string(), &ev_data.query_info).await;
        let cb = DbCallback::RemoveObject(AckRemoveObject::new(self.callback_id.to_string(), result));
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_guid t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_remove_object send_proxy is destory!");
        }
    }

    async fn do_get_object_info(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_get_object_info");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvGetObjectInfo>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_get_object_info p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let result = mongo_proxy.find(self.db.to_string(), self.collection.to_string(), &ev_data.query_info, ev_data.skip, ev_data.limit, ev_data.sort.to_string(), ev_data.ascending).await;
        let docs = match result {
            Err(err) => {
                error!("do_get_object_info find err:{}!", err);
                return
            },
            Ok(v) => v
        };
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            if docs.len() <= 0 {
                trace!("do_get_object_info doc is empty!");
                let doc = doc!{"__list__": docs};
                let mut bin: Vec<u8> = Vec::new();
                let _ = doc.to_writer(&mut bin);
                let wsize = (bin.len() + 2047) / 1024 * 1024;
                let cb = DbCallback::GetObjectInfo(AckGetObjectInfo::new(self.callback_id.to_string(), bin));
                let t = TBufferChannel::with_capacity(0, wsize);
                let (rd, wr) = match t.split() {
                    Ok(_t) => (_t.0, _t.1),
                    Err(_e) => {
                        error!("do_get_object_info t.split error {}", _e);
                        return;
                    }
                };
                let mut o_prot = TCompactOutputProtocol::new(wr);
                let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
                let _ = p_send.send(&rd.write_bytes()).await;
            }
            else {
                let mut idx = 0;
                while idx < docs.len() {
                    let idx1 = cmp::min(docs.len(), idx + 32);
                    let mut tmp: Vec<Document> = Vec::new();
                    tmp.clone_from_slice(&docs[idx..idx1]);
                    let doc = doc!{"__list__": tmp};
                    idx = idx1;
                    let mut bin: Vec<u8> = Vec::new();
                    let _ = doc.to_writer(&mut bin);
                    let wsize = (bin.len() + 2047) / 1024 * 1024;
                    let cb = DbCallback::GetObjectInfo(AckGetObjectInfo::new(self.callback_id.to_string(), bin));
                    let t = TBufferChannel::with_capacity(0, wsize);
                    let (rd, wr) = match t.split() {
                        Ok(_t) => (_t.0, _t.1),
                        Err(_e) => {
                            error!("do_get_object_info t.split error {}", _e);
                            return;
                        }
                    };
                    let mut o_prot = TCompactOutputProtocol::new(wr);
                    let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
                    let _ = p_send.send(&rd.write_bytes()).await;
                }
            }
            let cb = DbCallback::GetObjectInfoEnd(AckGetObjectInfoEnd::new(self.callback_id.to_string()));
            let t = TBufferChannel::with_capacity(0, 1024);
            let (rd, wr) = match t.split() {
                Ok(_t) => (_t.0, _t.1),
                Err(_e) => {
                    error!("do_get_object_info t.split error {}", _e);
                    return;
                }
            };
            let mut o_prot = TCompactOutputProtocol::new(wr);
            let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_get_object_info send_proxy is destory!");
        }
    }

    async fn do_get_object_count(&mut self, mongo_proxy:&mut MongoProxy) {
        trace!("begin do_get_object_count");
        let p_ev_data = self.ev_data.downcast_ref::<DBEvGetObjectCount>();
        let ev_data = match p_ev_data {
            None => {
                error!("do_get_object_count p_ev_data is null!");
                return;
            },
            Some(p) => p
        };
        let count = mongo_proxy.count(self.db.to_string(), self.collection.to_string(), &ev_data.query_info).await;
        let cb = DbCallback::GetObjectCount(AckGetObjectCount::new(self.callback_id.to_string(), count));
        let t = TBufferChannel::with_capacity(0, 16384);
        let (rd, wr) = match t.split() {
            Ok(_t) => (_t.0, _t.1),
            Err(_e) => {
                error!("do_get_object_count t.split error {}", _e);
                return;
            }
        };
        let mut o_prot = TCompactOutputProtocol::new(wr);
        let _ = DbCallback::write_to_out_protocol(&cb, &mut o_prot);
        if let Some(p) = self.send_proxy.upgrade() {
            let mut p_send = p.as_ref().lock().await;
            let _ = p_send.send(&rd.write_bytes()).await;
        }
        else {
            error!("do_get_object_count send_proxy is destory!");
        }
    }

    pub async fn do_event(&mut self, mongo_proxy:&mut MongoProxy) {
        match self.ev_type {
            DBEventType::EvGetGuid => self.do_get_guid(mongo_proxy).await,
            DBEventType::EvCreateObject => self.do_create_object(mongo_proxy).await,
            DBEventType::EvUpdataObject => self.do_updata_object(mongo_proxy).await,
            DBEventType::EvFindAndModify => self.do_find_and_modify(mongo_proxy).await,
            DBEventType::EvRemoveObject => self.do_remove_object(mongo_proxy).await,
            DBEventType::EvGetObjectInfo => self.do_get_object_info(mongo_proxy).await,
            DBEventType::EvGetObjectCount => self.do_get_object_count(mongo_proxy).await
        }
    }
}