# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
import random
import msgpack

from .base_entity import base_entity
from .callback import callback

class player(ABC, base_entity):
    def __init__(self, entity_type:str, entity_id:str, gate_name:str, conn_id:str) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.hub_request_callback:dict[str, Callable[[str, int, bytes],None]] = {}
        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}
        
        self.request_msg_cb_id = random.randint(100, 10011)
        self.client_request_callback:dict[str, Callable[[str, str, int, bytes],None]] = {}
        self.client_notify_callback:dict[str, Callable[[str, bytes],None]] = {}

        self.client_callback:dict[int, callback] = {}

        self.client_gate_name:str = gate_name
        self.client_conn_id:str = conn_id
        self.conn_hub_server:list[str] = []
        self.conn_client_gate:list[str] = []

        from app import app
        app().player_mgr.add_player(self)
        
    @abstractmethod
    def hub_info(self) -> dict:
        pass

    @abstractmethod
    def client_info(self) -> dict:
        pass
    
    def create_main_remote_entity(self):
        from app import app
        app().ctx.hub_call_client_create_remote_entity(self.client_gate_name, [], self.client_conn_id, self.entity_id, self.entity_type, msgpack.dumps(self.client_info()))
    
    def create_remote_entity(self, gate_name:str, conn_id:str):
        if gate_name not in self.conn_client_gate:
            self.conn_client_gate.append(gate_name)
        from app import app
        app().ctx.hub_call_client_create_remote_entity(gate_name, [conn_id], None, self.entity_id, self.entity_type, msgpack.dumps(self.client_info()))
    
    def create_remote_hub_entity(self, hub_name:str, service_name:str):
        if hub_name not in self.conn_hub_server:
            self.conn_hub_server.append(hub_name)
        from app import app
        app().ctx.create_service_entity(hub_name, service_name, self.entity_id, self.entity_type, self.hub_info())

    def handle_hub_request(self, source_hub:str, method:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, msg_cb_id, argvs)
        else:
            self.error("unhandle request method:{}, source:{}".format(method, source_hub))

    def handle_hub_notify(self, source_hub:str, method:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
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

    def del_client_callback(self, msg_cb_id:int) -> bool:
        if msg_cb_id not in self.client_callback:
            return False
        del self.client_callback[msg_cb_id]
        return True

    def handle_client_response(self, gate_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle._callback(argvs)
            del self.client_callback[msg_cb_id]
        else:
            self.error("unhandle response callback:{}, source:{}".format(msg_cb_id, gate_name))
    
    def handle_client_response_error(self, gate_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle.error(argvs)
            del self.client_callback[msg_cb_id]
        else:
            self.error("unhandle response error callback:{}, source:{}".format(msg_cb_id, gate_name))

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

    def call_client_request(self, method:str, argvs:bytes) -> int:
        from app import app
        msg_cb_id = self.request_msg_cb_id
        self.request_msg_cb_id += 1
        app().ctx.hub_call_client_rpc(self.client_gate_name, self.entity_id, msg_cb_id, method, argvs)
        return msg_cb_id
        
    def reg_client_callback(self, msg_cb_id:int, rsp:callback):
        self.client_callback[msg_cb_id] = rsp

    def call_client_response(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_client_rsp(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_response_error(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.hub_call_client_err(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_main_notify(self, method:str, argvs:bytes):
        from app import app
        app().ctx.hub_call_client_ntf(self.client_gate_name, self.client_conn_id, method, argvs)

    def call_client_mutilcast(self, method:str, argvs:bytes):
        from app import app
        for gate_name in self.conn_client_gate:
            app().ctx.hub_call_client_ntf(gate_name, None, method, argvs)

class player_event_handle(ABC):
    @abstractmethod
    def player_offline(self, _player:player) -> dict:
        pass

class player_manager(object):
    def __init__(self, player_event_handle:player_event_handle):
        self.__player_event_handle__ = player_event_handle
        self.players:dict[str, player] = {}
        self.conn_id_players:dict[str, list[player]] = {}
        
    def add_player(self, _player:player):
        self.players[_player.entity_id] = _player
        
        if not _player.client_conn_id in self.conn_id_players:
            self.conn_id_players[_player.client_conn_id] = []
        self.conn_id_players[_player.client_conn_id].append(_player)

    def get_player(self, entity_id:str) -> player:
        return self.players[entity_id]
    
    def update_player_conn(self, entity_id:str, is_main:bool, is_reconnect:bool, gate_name:str, conn_id:str) -> bool:
        _player = self.players[entity_id]
        if not _player:
            return False
        
        _p_list = self.conn_id_players[_player.client_conn_id]
            
        if gate_name not in _player.conn_client_gate:
            _player.conn_client_gate.append(gate_name)
        
        _player.client_conn_id = conn_id
        _player.client_gate_name = gate_name
        
        for _p in _p_list:
            if gate_name not in _p.conn_client_gate:
                _p.conn_client_gate.append(gate_name)
        
            _p.client_conn_id = conn_id
            _p.client_gate_name = gate_name
            
        self.conn_id_players[_player.client_conn_id] = _p_list
        
        from app import app
        if is_reconnect:
            app().ctx.hub_call_client_refresh_entity(gate_name, conn_id, is_main, _player.entity_id, _player.entity_type, msgpack.dumps(_player.info()))
        else:
            if is_main:
                app().ctx.hub_call_client_create_remote_entity(gate_name, [], conn_id, _player.entity_id, _player.entity_type, msgpack.dumps(_player.info()))
            else:
                app().ctx.hub_call_client_create_remote_entity(gate_name, [conn_id], None, _player.entity_id, _player.entity_type, msgpack.dumps(_player.info()))
        
        return True
    
    def get_player_by_conn_id(self, conn_id:str) -> list[player]:
        return self.conn_id_players[conn_id]
    
    def del_player(self, entity_id:str):
        del self.players[entity_id]
        
    def player_offline(self, conn_id:str):
        _player_list = self.get_player_by_conn_id(conn_id)
        for _player in _player_list:
            self.__player_event_handle__.player_offline(_player)