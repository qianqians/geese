# -*- coding: UTF-8 -*-
from threading import Timer
from collections.abc import Callable
from .pyhub import HubContext

def transfer_timeout(new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str):
    from app import app
    _t = app().ctx.transfer_timeout[new_conn_id]
    if _t!= None:
        app().ctx.transfer_timeout.pop(new_conn_id)
        app().login_handle.reconnect(new_gate_name, new_conn_id, sdk_uuid, token)

class context(object):
    def __init__(self, cfg_file:str) -> None:
        self.ctx = HubContext(cfg_file)
        self.flush_hub_host_cache()
        self.transfer_timeout:dict[str, Timer] = {}

    def hub_name(self) -> str:
        return self.ctx.hub_name()
    
    def save_time_interval(self) -> int:
        self.ctx.save_time_interval()

    def migrate_time_interval(self) -> int:
        self.ctx.migrate_time_interval()

    def log(self, level:str, content:str):
        self.ctx.log(level, content)
        
    def register_service(self, service:str):
        self.ctx.register_service(service)
        
    def set_health_state(self, status:bool):
        self.ctx.set_health_state(status)
        
    def entry_dbproxy_service(self) -> str:
        return self.ctx.entry_dbproxy_service()
        
    async def entry_hub_service(self, service_name:str) -> str:
        return await self.ctx.entry_hub_service(service_name)
    
    async def entry_direct_hub_server(self, hub_name:str):
        return await self.ctx.entry_direct_hub_server(hub_name)
    
    def check_connect_hub_server(self, hub_name:str) -> bool:
        return self.ctx.check_connect_hub_server(hub_name)
    
    async def entry_gate_service(self, gate_name:str, gate_host:str):
        return await self.ctx.entry_gate_service(gate_name, gate_host)
    
    def gate_host(self, gate_name:str):
        return self.ctx.gate_host(gate_name)
    
    def set_time_offset(self, offset_time:int):
        self.ctx.set_time_offset(offset_time)

    def utc_unix_time_with_offset(self) -> int :
        self.ctx.utc_unix_time_with_offset()
    
    def flush_hub_host_cache(self):
        __tick__ = Timer(10, self.flush_hub_host_cache)
        __tick__.start()
        return self.ctx.flush_hub_host_cache()
        
    def reg_hub_to_hub(self, hub_name:str) -> bool:
        return self.ctx.reg_hub_to_hub(hub_name)
    
    def query_service(self, hub_name:str, service_name:str) -> bool:
        return self.ctx.query_service(hub_name, service_name)
    
    def create_service_entity(self, is_migrate: bool, hub_name:str, service_name:str, entity_id:str, entity_type:str, argvs:bytes) -> bool:
        return self.ctx.create_service_entity(is_migrate, hub_name, service_name, entity_id, entity_type, argvs)
    
    def forward_client_request_service(self, hub_name:str, service_name:str, gate_name:str, gate_host:str, conn_id:str, player_id:str) -> bool:
        return self.ctx.forward_client_request_service(hub_name, service_name, gate_name, gate_host, conn_id, player_id)
    
    def hub_call_hub_rpc(self, hub_name:str, entity_id:str, msg_cb_id:int, method:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_hub_rpc(hub_name, entity_id, msg_cb_id, method, argvs)
    
    def hub_call_hub_rsp(self, hub_name:str, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.hub_call_hub_rsp(hub_name, entity_id, msg_cb_id, argvs)
        
    def hub_call_hub_err(self, hub_name:str, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.hub_call_hub_err(hub_name, entity_id, msg_cb_id, argvs) 
    
    def hub_call_hub_ntf(self, hub_name:str, entity_id:str, method:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_hub_ntf(hub_name, entity_id, method, argvs)
    
    def hub_call_hub_wait_migrate_entity(self, hub_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_hub_wait_migrate_entity(hub_name, entity_id)
    
    def hub_call_create_migrate_entity(self, hub_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_create_migrate_entity(hub_name, entity_id)
    
    def hub_call_hub_migrate_entity(self, hub_name:str, service_name:str, entity_type:str, entity_id:str, main_gate_name:str, main_conn_id:str, gates:list[str], hubs:list[str], argvs:bytes) -> bool:
        return self.ctx.hub_call_hub_migrate_entity(hub_name, service_name, entity_type, entity_id, main_gate_name, main_conn_id, gates, hubs, argvs)
    
    def hub_call_hub_migrate_entity_complete(self, hub_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_hub_migrate_entity_complete(hub_name, entity_id)
  
    def hub_call_client_create_remote_entity(self, gate_name:str, is_migrate: bool, conn_id:list[str], main_conn_id:str, entity_id:str, entity_type:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_create_remote_entity(gate_name, is_migrate, conn_id, main_conn_id, entity_id, entity_type, argvs)
    
    def hub_call_client_delete_remote_entity(self, gate_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_client_delete_remote_entity(gate_name, entity_id)
    
    def hub_call_client_remove_remote_entity(self, gate_name:str, entity_id:str, conn_id:str) -> bool:
        return self.ctx.hub_call_client_remove_remote_entity(gate_name, entity_id, conn_id)
    
    def hub_call_client_refresh_entity(self, gate_name:str, is_migrate: bool, conn_id:str, is_main:bool, entity_id:str, entity_type:str, argvs:bytes):
        return self.ctx.hub_call_client_refresh_entity(gate_name, is_migrate, conn_id, is_main, entity_id, entity_type, argvs)
    
    def hub_call_client_rpc(self, gate_name:str, entity_id:str, msg_cb_id:int, method:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_rpc(gate_name, entity_id, msg_cb_id, method, argvs)
    
    def hub_call_client_rsp(self, gate_name:str, conn_id:str, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_rsp(gate_name, conn_id, entity_id, msg_cb_id, argvs)
    
    def hub_call_client_err(self, gate_name:str, conn_id:str, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_err(gate_name, conn_id, entity_id, msg_cb_id, argvs)
    
    def hub_call_client_ntf(self, gate_name:str, conn_id:str, entity_id:str, method:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_ntf(gate_name, conn_id, entity_id, method, argvs)
    
    def hub_call_client_global(self, gate_name:str, method:str, argvs:bytes) -> bool:
        return self.ctx.hub_call_client_global(gate_name, method, argvs)
    
    def hub_call_kick_off_client(self, gate_name:str, conn_id:str, prompt_info:str) -> bool:
        return self.ctx.hub_call_kick_off_client(gate_name, conn_id, prompt_info)
    
    def hub_call_kick_off_client_complete(self, gate_name:str, conn_id:str) -> bool:
        return self.ctx.hub_call_kick_off_client_complete(gate_name, conn_id)

    def hub_call_replace_client(self, old_gate_name:str, old_conn_id:str, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str, is_replace:bool, prompt_info:str) -> bool:
        from app import app
        self.transfer_timeout[new_conn_id] = Timer(1000, lambda : transfer_timeout(new_gate_name, new_conn_id, sdk_uuid, token))
        return self.ctx.hub_call_transfer_client(old_gate_name, old_conn_id, new_gate_name, new_conn_id, is_replace, prompt_info)
    
    def hub_call_gate_wait_migrate_entity(self, gate_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_gate_wait_migrate_entity(gate_name, entity_id)
    
    def hub_call_gate_migrate_entity_complete(self, gate_name:str, entity_id:str) -> bool:
        return self.ctx.hub_call_gate_migrate_entity_complete(gate_name, entity_id)
    
    def get_guid(self, dbproxy_name:str, db:str, collection:str, callback_id:str) -> bool:
        return self.ctx.get_guid(dbproxy_name, db, collection, callback_id)
    
    def create_object(self, dbproxy_name:str, db:str, collection:str, callback_id:str, object_info:bytes) -> bool:
        return self.ctx.create_object(dbproxy_name, db, collection, callback_id, object_info)
    
    def update_object(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes, updata_info:bytes, _upsert:bool) -> bool:
        return self.ctx.update_object(dbproxy_name, db, collection, callback_id, query_info, updata_info, _upsert)
    
    def find_and_modify(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes, updata_info:bytes, _new:bool, _upsert:bool) -> bool:
        return self.ctx.find_and_modify(dbproxy_name, db, collection, callback_id, query_info, updata_info, _new, _upsert)
    
    def remove_object(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes) -> bool:
        return self.ctx.remove_object(dbproxy_name, db, collection, callback_id, query_info)
    
    def get_object_info(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes, skip:int, limit:int, sort:str, ascending:bool) -> bool:
        return self.ctx.get_object_info(dbproxy_name, db, collection, callback_id, query_info, skip, limit, sort, ascending)
    
    def get_object_count(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes) -> bool:
        return self.ctx.get_object_count(dbproxy_name, db, collection, callback_id, query_info)
    
    def get_object_one(self, dbproxy_name:str, db:str, collection:str, callback_id:str, query_info:bytes) -> bool:
        return self.ctx.get_object_one(dbproxy_name, db, collection, callback_id, query_info)