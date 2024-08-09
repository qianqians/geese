use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::BTreeMap;
use proto::hub::HubService;

use crate::conn_manager::ConnManager;

#[derive(Clone)]
pub struct CacheMigrateMsg {
    pub conn_mgr: Arc<Mutex<ConnManager>>,
    pub msg: HubService,
}

#[derive(Clone)]
pub struct Entity {
    entity_id: String,
    hub_name: String,
    main_conn_id: Option<String>,
    conn_ids: Vec<String>,
    is_migrate: bool,
    cache_migrate_msg: Vec<CacheMigrateMsg>
}

impl Entity {
    pub fn new(_entity_id: String, _hub_name: String) -> Entity {
        Entity {
            entity_id: _entity_id,
            hub_name: _hub_name,
            main_conn_id: None,
            conn_ids: vec![],
            is_migrate: false,
            cache_migrate_msg: vec![]
        }
    }

    pub fn set_main_conn_id(&mut self, id: Option<String>) {
        self.main_conn_id = id
    }

    pub fn get_main_conn_id(&self) -> Option<String> {
        self.main_conn_id.clone()
    }

    pub fn add_conn_id(&mut self, id: String) {
        self.conn_ids.push(id)
    }

    pub fn delete_conn_id(&mut self, conn_id: &String) {
        for (index, item) in self.conn_ids.iter().enumerate() {
            if *item == *conn_id {
                self.conn_ids.remove(index);
                break;
            }
        }
    }

    pub fn get_conn_ids(&self) -> &Vec<String> {
        &self.conn_ids
    }

    pub fn get_hub_name(&self) -> &String {
        &self.hub_name
    }

    pub fn is_migrate(&self) -> bool {
        self.is_migrate
    }

    pub fn cache_migrate_msg(&mut self, msg: CacheMigrateMsg) {
        self.cache_migrate_msg.push(msg)
    }

    pub async fn do_cache_msg(&mut self) {
        for msg in &self.cache_migrate_msg {
            let mut delete_hub_name: Option<String> = None; 
            {
                let mut _conn_mgr = msg.conn_mgr.as_ref().lock().await;
                let hub_name = self.get_hub_name();
                if let Some(_hub_arc) = _conn_mgr.get_hub_proxy(hub_name) {
                    let mut _hub = _hub_arc.as_ref().lock().await;
                    if !_hub.send_hub_msg(msg.msg.clone()).await {
                        delete_hub_name = Some(hub_name.clone());
                    }
                }
            }

            if let Some(name) = delete_hub_name {
                let mut _conn_mgr = msg.conn_mgr.as_ref().lock().await;
                _conn_mgr.delete_hub_proxy(&name);
            }
        }
    }

}

pub struct EntityManager {
    entities: BTreeMap<String, Entity>,
    client_hub_map: BTreeMap<String, Vec<String>>
}

impl EntityManager {
    pub fn new() -> EntityManager {
        EntityManager {
            entities: BTreeMap::new(),
            client_hub_map: BTreeMap::new()
        }
    }

    pub fn update_entity(&mut self, e: Entity) {
        let entity_id = e.entity_id.clone();
        let main_conn_id_opt = e.main_conn_id.clone();
        let hub_name = e.hub_name.clone();
        self.entities.insert(entity_id, e);
        if let Some(conn_id) = main_conn_id_opt {
            let client_hub_vec = match self.client_hub_map.get_mut(&conn_id) {
                None => {
                    let _conn_id_tmp = conn_id.clone();
                    self.client_hub_map.insert(conn_id, Vec::new());
                    self.client_hub_map.get_mut(&_conn_id_tmp).unwrap()
                },
                Some(_vec) => _vec
            };
            client_hub_vec.push(hub_name)
        }
    }

    pub fn get_entity_mut(&mut self, entity_id: &String) -> Option<&mut Entity> {
        self.entities.get_mut(entity_id)
    }

    pub fn get_entity(&self, entity_id: &String) -> Option<&Entity> {
        self.entities.get(entity_id)
    }

    pub fn delete_entity(&mut self, entity_id: &String) -> Option<Entity> {
        self.entities.remove(entity_id)
    }

    pub fn delete_client(&mut self, conn_id: &String) -> Option<Vec<String>>{
        self.client_hub_map.remove(conn_id)
    }
}