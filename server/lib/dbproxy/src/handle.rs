use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{trace, error};

use thrift::protocol::{TCompactInputProtocol, TSerializable};
use thrift::transport::TBufferChannel;

use proto::dbproxy::{
    DbEvent, 
    GetGuidEvent, 
    CreateObjectEvent, 
    UpdateObjectEvent, 
    FindAndModifyEvent, 
    RemoveObjectEvent, 
    GetObjectInfoEvent, 
    GetObjectCountEvent
};

use net::NetWriter;
use mongo::MongoProxy;
use queue::Queue;

use crate::db;

pub struct DBProxyHubMsgHandle {
    proxy: MongoProxy,
    queue: Queue<Box<db::DBEvent>>
}

fn deserialize(data: Vec<u8>) -> Result<DbEvent, Box<dyn std::error::Error>> {
    trace!("deserialize begin!");
    let mut t = TBufferChannel::with_capacity(data.len(), 0);
    let _ = t.set_readable_bytes(&data);
    let mut i_prot = TCompactInputProtocol::new(t);
    let ev_data = DbEvent::read_from_in_protocol(&mut i_prot)?;
    Ok(ev_data)
}

impl DBProxyHubMsgHandle {
    pub async fn new(mongo_proxy:MongoProxy) -> Result<Arc<Mutex<DBProxyHubMsgHandle>>, Box<dyn std::error::Error>> {
        let mut _db_server = Arc::new(Mutex::new(DBProxyHubMsgHandle {
            proxy: mongo_proxy,
            queue: Queue::new(), 
        }));
        Ok(_db_server)
    }

    pub async fn on_event(_handle: Arc<Mutex<DBProxyHubMsgHandle>>, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>, data: Vec<u8>) {
        trace!("on_event begin!");
        let ev = match deserialize(data) {
            Err(e) => {
                error!("DBProxyThriftServer on_event err:{}", e);
                return;
            }
            Ok(d) => d
        };
        
        let mut _p = _handle.as_ref().lock().await;
        match ev {
            DbEvent::RegHub(_data) => {
                trace!("on_RegHub hub:{}!", _data.hub_name.unwrap());
            },
            DbEvent::GetGuid(_data) => {
                _p.do_get_guid(_data, rsp).await;
            },
            DbEvent::CreateObject(_data) => {
                _p.do_create_object(_data, rsp).await;
            },
            DbEvent::UpdateObject(_data) => {
                _p.do_update_object(_data, rsp).await;
            },
            DbEvent::FindAndModify(_data) => {
                _p.do_find_and_modify(_data, rsp).await;
            },
            DbEvent::RemoveObject(_data) => {
                _p.do_remove_object(_data, rsp).await;
            },
            DbEvent::GetObjectInfo(_data) => {
                _p.do_get_object_info(_data, rsp).await;
            },
            DbEvent::GetObjectCount(_data) => {
                _p.do_get_object_count(_data, rsp).await;
            }
        }
    }

    pub async fn poll(_handle: Arc<Mutex<DBProxyHubMsgHandle>>) {
        loop {
            let mut _self: tokio::sync::MutexGuard<'_, DBProxyHubMsgHandle> = _handle.as_ref().lock().await;
            let opt_ev_data = _self.queue.deque();
            match opt_ev_data {
                None => break,
                Some(ev_data) => {
                    let mut mut_ev_data = ev_data;
                    mut_ev_data.do_event(&mut _self.proxy).await;
                }
            }
        }
    }

    async fn do_get_guid(&mut self, _data: GetGuidEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_get_guid begin!");

        let ev_data = db::DBEvGetGuid::new();
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event GetGuid db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event GetGuid collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event GetGuid callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp),  db::DBEventType::EvGetGuid, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_create_object(&mut self, _data: CreateObjectEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_create_object begin!");

        let object_info = match _data.object_info {
            None => {
                error!("DBProxyThriftServer do_event CreateObjectEvent object_info is None!");
                return;
            },
            Some(_object_info) => _object_info
        };
        let ev_data = db::DBEvCreateObject::new(object_info);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event CreateObjectEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event CreateObjectEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event CreateObjectEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvCreateObject, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_update_object(&mut self, _data: UpdateObjectEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_update_object begin!");

        let query_info = match _data.query_info {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent object_info is None!");
                return;
            },
            Some(_query_info) => _query_info
        };
        let updata_info = match _data.updata_info {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent updata_info is None!");
                return;
            },
            Some(_updata_info) => _updata_info
        };
        let _upsert = match _data._upsert {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent _upsert is None!");
                return;
            },
            Some(_upsert) => _upsert
        };
        let ev_data = db::DBEvUpdataObject::new(query_info, updata_info, _upsert);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event UpdateObjectEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvUpdataObject, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_find_and_modify(&mut self, _data: FindAndModifyEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_find_and_modify begin!");

        let query_info = match _data.query_info {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent query_info is None!");
                return;
            },
            Some(_query_info) => _query_info
        };
        let updata_info = match _data.updata_info {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent updata_info is None!");
                return;
            },
            Some(_updata_info) => _updata_info
        };
        let _new = match _data._new {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent _new is None!");
                return;
            },
            Some(_new) => _new
        };
        let _upsert = match _data._upsert {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent _upsert is None!");
                return;
            },
            Some(_upsert) => _upsert
        };
        let ev_data = db::DBEvFindAndModify::new(query_info, updata_info, _new, _upsert);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event FindAndModifyEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvFindAndModify, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_remove_object(&mut self, _data: RemoveObjectEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_remove_object begin!");

        let query_info = match _data.query_info {
            None => {
                error!("DBProxyThriftServer do_event RemoveObjectEvent query_info is None!");
                return;
            },
            Some(_query_info) => _query_info
        };
        let ev_data = db::DBEvRemoveObject::new(query_info);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event RemoveObjectEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event RemoveObjectEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event RemoveObjectEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvRemoveObject, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_get_object_info(&mut self, _data: GetObjectInfoEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_get_object_info begin!");

        let query_info = match _data.query_info {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent query_info is None!");
                return;
            },
            Some(_query_info) => _query_info
        };
        let skip = match _data.skip {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent skip is None!");
                return;
            },
            Some(_skip) => _skip
        };
        let limit = match _data.limit {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent limit is None!");
                return;
            },
            Some(_limit) => _limit
        };
        let sort = match _data.sort {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent sort is None!");
                return;
            },
            Some(_sort) => _sort
        };
        let ascending = match _data.ascending {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent ascending is None!");
                return;
            },
            Some(_ascending) => _ascending
        };
        let ev_data = db::DBEvGetObjectInfo::new(query_info, skip, limit, sort, ascending);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event GetObjectInfoEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvGetObjectInfo, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

    async fn do_get_object_count(&mut self, _data: GetObjectCountEvent, rsp: Arc<Mutex<Box<dyn NetWriter + Send + 'static>>>) {
        trace!("do_get_object_count begin!");

        let query_info = match _data.query_info {
            None => {
                error!("DBProxyThriftServer do_event GetObjectCountEvent query_info is None!");
                return;
            },
            Some(_query_info) => _query_info
        };
        let ev_data = db::DBEvGetObjectCount::new(query_info);
        let db = match _data.db {
            None => {
                error!("DBProxyThriftServer do_event GetObjectCountEvent db is None!");
                return;
            },
            Some(_db) => _db
        };
        let collection = match _data.collection {
            None => {
                error!("DBProxyThriftServer do_event GetObjectCountEvent collection is None!");
                return;
            },
            Some(_collection) => _collection
        };
        let callback_id = match _data.callback_id {
            None => {
                error!("DBProxyThriftServer do_event GetObjectCountEvent callback_id is None!");
                return;
            },
            Some(_callback_id) => _callback_id
        };
        let ev = db::DBEvent::new(Arc::downgrade(&rsp), db::DBEventType::EvGetObjectCount, db, collection, callback_id, Box::new(ev_data));
        self.queue.enque(Box::new(ev));
    }

}