# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
from .base_entity import base_entity
import msgpack

class entity(ABC, base_entity):
    def __init__(self, service_name:str, entity_type:str, entity_id:str, is_dynamic:bool) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.hub_request_callback:dict[str, Callable[[str, int, bytes],None]] = {}
        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}
        
        self.client_request_callback:dict[str, Callable[[str, str, int, bytes],None]] = {}
        self.client_notify_callback:dict[str, Callable[[str, bytes],None]] = {}
        
        self.service_name = service_name
        self.conn_hub_server:list[str] = []
        self.conn_client_gate:list[str] = []
        
        self.is_dynamic = is_dynamic
        if is_dynamic:
            self.wait_lock_migrate_svr:list[str] = []

        from app import app
        app().entity_mgr.add_entity(self)

    @abstractmethod
    def full_info(self) -> dict:
        pass
        
    @abstractmethod
    def hub_info(self) -> dict:
        pass

    @abstractmethod
    def client_info(self) -> dict:
        pass
    
    def start_migrate_entity(self):
        from app import app
        for hub in self.conn_hub_server:
            app().ctx.hub_call_hub_wait_migrate_entity(hub, self.entity_id)
            self.wait_lock_migrate_svr.append(hub)
        for gate in self.conn_client_gate:
            app().ctx.hub_call_gate_wait_migrate_entity(gate, self.entity_id)
            self.wait_lock_migrate_svr.append(gate)
            
    def check_migrate_entity_lock(self, svr:str):
        if svr not in self.wait_lock_migrate_svr:
            return
        self.wait_lock_migrate_svr.remove(svr)
        if len(self.wait_lock_migrate_svr) <= 0:
            from app import app
            migrate_hub = app().ctx.entry_hub_service(self.service_name)
            app().ctx.hub_call_hub_migrate_entity(migrate_hub, self.service_name, self.entity_type, self.entity_id, self.conn_client_gate, self.conn_hub_server, self.full_info())
    
    def create_remote_entity(self, gate_name:str, conn_id:str):
        if gate_name not in self.conn_client_gate:
            self.conn_client_gate.append(gate_name)
        from app import app
        app().ctx.hub_call_client_create_remote_entity(gate_name, [conn_id], "", self.entity_id, self.entity_type, msgpack.dumps(self.client_info()))

    def create_remote_hub_entity(self, hub_name:str):
        if hub_name not in self.conn_hub_server:
            self.conn_hub_server.append(hub_name)
        from app import app
        app().ctx.create_service_entity(hub_name, self.service_name, self.entity_id, self.entity_type, msgpack.dumps(self.hub_info()))

    def handle_hub_request(self, source_hub:str, method:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, msg_cb_id, argvs)
        else:
            self.error("unhandle request method:{}, source:{}".format(method, source_hub))

    def handle_hub_notify(self, source_hub:str, method:str, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, source_hub))

    def reg_hub_request_callback(self, method:str, callback:Callable[[str, int, bytes],None]):
        self.hub_request_callback[method] = callback

    def reg_hub_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.hub_notify_callback[method] = callback

    def call_hub_response(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_hub_rsp(hub_name, self.entity_id, msg_cb_id, argvs)

    def call_hub_response_error(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_hub_err(hub_name, self.entity_id, msg_cb_id, argvs)

    def call_hub_notify(self, method:str, argvs:bytes):
        from app import app
        for hub_name in self.conn_hub_server:
            app().ctx.hub_call_hub_ntf(hub_name, method, argvs)

    def handle_client_request(self, gate_name:str, conn_id:str, method:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_request_callback[method]
        if _call_handle != None:
            _call_handle(gate_name, conn_id, msg_cb_id, argvs)
        else:
            self.error("unhandle request method:{}, source:({}, {})".format(method, gate_name, conn_id))

    def handle_client_notify(self, gate_name:str, method:str, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(gate_name, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, gate_name))

    def reg_client_request_callback(self, method:str, callback:Callable[[str, str, int, bytes],None]):
        self.client_request_callback[method] = callback

    def reg_client_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.client_notify_callback[method] = callback

    def call_client_response(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_client_rsp(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_response_error(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_client_err(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_mutilcast(self, method:str, argvs:bytes):
        from app import app
        for gate_name in self.conn_client_gate:
            app().ctx.hub_call_client_ntf(gate_name, None, method, argvs)
    
class entity_manager(object):
    def __init__(self):
        self.entities:dict[str, entity] = {}
        
    def add_entity(self, _entity:entity):
        self.entities[_entity.entity_id] = _entity

    def update_entity_conn(self, entity_id:str, is_reconnect:bool, gate_name:str, conn_id:str) -> bool:
        _entity = self.entities[entity_id]
        if not _entity:
            return False
        
        if gate_name not in _entity.conn_client_gate:
            _entity.conn_client_gate.append(gate_name)
        
        from app import app
        if is_reconnect:
            app().ctx.hub_call_client_refresh_entity(gate_name, conn_id, False, _entity.entity_id, _entity.entity_type, msgpack.dumps(_entity.info()))
        else:
            app().ctx.hub_call_client_create_remote_entity(gate_name, [conn_id], None, _entity.entity_id, _entity.entity_type, msgpack.dumps(_entity.info()))
                
        return True
        
    def get_entity(self, entity_id) -> entity:
        if entity_id in self.entities:
            return self.entities[entity_id]
        return None
    
    def del_entity(self, entity_id):
        if entity_id in self.entities:
            del self.entities[entity_id]