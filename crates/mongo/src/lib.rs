use mongodb::options::{UpdateOptions, FindOneAndUpdateOptions, IndexOptions, ReturnDocument, FindOptions};
use mongodb::{Client, options::ClientOptions, IndexModel};
use mongodb::bson::{doc, Document};
use futures::stream::TryStreamExt;

use tracing::{trace, error};

pub struct MongoProxy {
    client : Client
}

impl MongoProxy {
    pub async fn new(url:String) -> Result<MongoProxy, Box<dyn std::error::Error>> {
        let _client_options = ClientOptions::parse(url).await?;
        let _client = Client::with_options(_client_options)?;
        Ok(MongoProxy{ client : _client })
    }

    pub async fn create_index(&mut self, db: String, collection: String, key: String, is_unique: bool) -> bool {
        trace!("create_index db:{}, collection:{}, key:{}!", db, collection, key);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let _options = IndexOptions::builder().unique(is_unique).build();
        let _index = IndexModel::builder()
            .keys(doc!{ key: 1})
            .options(_options)
            .build();
        let result = _collection.create_index(_index, None).await;
        match result {
            Ok(_v) => return true,
            Err(_e) => {
                error!("create_index error:{}", _e);
                return false
            }
        }
    }

    pub async fn check_int_guid(&mut self, db: String, collection: String, initial_guid: u32) -> bool {
        trace!("check_int_guid db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let result = _collection.insert_one(doc!{ "GuidIndexKey": "__guid__", "guid": initial_guid }, None).await;
        match result {
            Ok(_v) => return true,
            Err(_e) => {
                error!("check_int_guid error:{}", _e);
                return false
            }
        }
    }

    pub async fn save(&mut self, db: String, collection: String, data: &Vec<u8>) -> bool {
        trace!("save db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _bin = std::io::Cursor::new(data);
        let _doc_result = mongodb::bson::Document::from_reader(&mut _bin); 
        let _doc = match _doc_result {
            Ok(v) => v,
            Err(_e) => {
                error!("save _doc_result error:{}", _e);
                return false
            }
        };

        let result = _collection.insert_one(_doc, None).await;
        match result {
            Ok(_v) => return true,
            Err(_e) => {
                error!("save insert_one error:{}", _e);
                return false
            }
        }
    }

    pub async fn update(&mut self, db: String, collection: String, query: &Vec<u8>, update: &Vec<u8>, is_upsert: bool) -> bool {
        trace!("update db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _query_bin = std::io::Cursor::new(query);
        let _query_result = mongodb::bson::Document::from_reader(&mut _query_bin); 
        let _query = match _query_result {
            Ok(v) => v,
            Err(_e) => {
                error!("update _query_result error:{}", _e);
                return false
            }
        };

        let mut _update_bin = std::io::Cursor::new(update);
        let _update_result = mongodb::bson::Document::from_reader(&mut _update_bin); 
        let _update = match _update_result {
            Ok(v) => v,
            Err(_e) => {
                error!("update _update_result error:{}", _e);
                return false
            }
        };

        let _opts = UpdateOptions::builder().upsert(is_upsert).build();
        let result = _collection.update_one(_query, _update, _opts).await;
        match result {
            Ok(_v) => return true,
            Err(_e) => {
                error!("update update_one error:{}", _e);
                return false
            }
        }
    }

    pub async fn find_and_modify(&mut self, db: String, collection: String, query: &Vec<u8>, update: &Vec<u8>, _new: bool, _upsert: bool) -> Result<Option<Document>, Box<dyn std::error::Error>> {
        trace!("find_and_modify db:{}, collection:{}!", db, collection);
        
        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _query_bin = std::io::Cursor::new(query);
        let _query = mongodb::bson::Document::from_reader(&mut _query_bin)?; 

        let mut _update_bin = std::io::Cursor::new(update);
        let _update = mongodb::bson::Document::from_reader(&mut _update_bin)?; 

        let _return_document = if _new { ReturnDocument::After } else { ReturnDocument::Before };
        let _opts = FindOneAndUpdateOptions::builder()
            .projection(doc! { "_id": 0 })
            .return_document(_return_document)
            .upsert(_upsert)
            .build();
        let result = _collection.find_one_and_update(_query, _update, _opts).await?;
        Ok(result)
    }

    pub async fn find(&mut self, db: String, collection: String, query: &Vec<u8>, skip: u32, limit: u32, sort: String, _ascending: bool) -> Result<Vec<Document>, Box<dyn std::error::Error>>  {
        trace!("find db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _query_bin = std::io::Cursor::new(query);
        let _query = mongodb::bson::Document::from_reader(&mut _query_bin)?;
        
        let ascending = if _ascending { 1 } else { -1 };
        let mut _opts_builder = FindOptions::builder().projection(doc! { "_id": 0 });
        let _opts: FindOptions;
        if !sort.is_empty() {
            _opts = _opts_builder.sort(doc! { sort: ascending }).skip(skip as u64).limit(limit as i64).build();
        }
        else {
            _opts = _opts_builder.skip(skip as u64).limit(limit as i64).build();
        }
        let mut _cursor = _collection.find(_query, _opts).await?;
        let vec: Vec<_> = _cursor.try_collect().await?;
        Ok(vec)
    }

    pub async fn count(&mut self, db: String, collection: String, query: &Vec<u8>) -> i32 {
        trace!("count db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _query_bin = std::io::Cursor::new(query);
        let _query_result = mongodb::bson::Document::from_reader(&mut _query_bin);
        let _query = match _query_result {
            Ok(v) => v,
            Err(_e) => {
                error!("count _query_result error:{}", _e);
                return -1
            }
        };

        let result = _collection.count_documents(_query, None).await;
        match result {
            Ok(v) => return v as i32,
            Err(_e) =>  {
                error!("count count_documents error:{}", _e);
                return -1
            }
        };
    }

    pub async fn remove(&mut self, db: String, collection: String, query: &Vec<u8>) -> bool {
        trace!("remove db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let mut _query_bin = std::io::Cursor::new(query);
        let _query_result = mongodb::bson::Document::from_reader(&mut _query_bin);
        let _query = match _query_result {
            Ok(v) => v,
            Err(_e) => {
                error!("remove _query_result error:{}", _e);
                return false
            }
        };

        let result = _collection.delete_many(_query, None).await;
        match result {
            Ok(_v) => return true,
            Err(_e) => {
                error!("remove delete_one error:{}", _e);
                return false
            }
        };
    }

    pub async fn get_guid(&mut self, db: String, collection: String) -> i64 {
        trace!("get_guid db:{}, collection:{}!", db, collection);

        let _db = self.client.database(&db);
        let _collection  = _db.collection::<Document>(&collection);

        let _query = doc!{ "GuidIndexKey": "__guid__"};
        let _update = doc!{ "$inc": doc!{ "guid": 1 } };

        let _return_document = ReturnDocument::Before;
        let _opts = FindOneAndUpdateOptions::builder()
            .return_document(_return_document)
            .build();
        let result = _collection.find_one_and_update(_query, _update, _opts).await;
        let _doc = match result {
            Ok(_v) => _v,
            Err(_e) => {
                error!("get_guid find_one_and_update error:{}", _e);
                return -1
            }
        };
        let _guid_doc = match _doc {
            None => {
                error!("get_guid _guid_doc None!");
                return -1
            }
            Some(_doc) => _doc
        };
        let guid_result = _guid_doc.get_i64("guid");
        match guid_result {
            Ok(_v) => return _v,
            Err(_e) => {
                error!("get_guid _guid_doc.get_i64 error:{}", _e);
                return -1
            }
        }
    }
}