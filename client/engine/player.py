# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
import random

from .base_entity import base_entity
from .callback import callback

class player(ABC, base_entity):
    def __init__(self, entity_type:str, entity_id:str) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.request_msg_cb_id = random.randint(100, 10011)
        self.hub_request_callback:dict[str, Callable[[str, int, bytes],None]] = {}
        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}

        self.hub_callback:dict[int, callback] = {}
    
        from app import app
        app().player_mgr.add_player(self)

    @abstractmethod
    def update_player(self, argvs: dict):
        pass

    def handle_hub_request(self, method:str, hub_name, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(hub_name, msg_cb_id, argvs)
        else:
            print("unhandle request method:{}".format(method))

    def del_callback(self, msg_cb_id:int) -> bool:
        if msg_cb_id not in self.hub_callback:
            return False
        del self.hub_callback[msg_cb_id]
        return True

    def handle_hub_response(self, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle._callback(argvs)
            del self.hub_callback[msg_cb_id]
        else:
            print("unhandle response callback:{}".format(msg_cb_id))
    
    def handle_hub_response_error(self, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle.error(argvs)
            del self.hub_callback[msg_cb_id]
        else:
            print("unhandle response error callback:{}".format(msg_cb_id))

    def handle_hub_notify(self, method:str, hub_name:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
        if _call_handle != None:
            _call_handle(hub_name, argvs)
        else:
            print("unhandle notify method:{}".format(method))

    def reg_hub_request_callback(self, method:str, callback:Callable[[int, bytes],None]):
        self.hub_request_callback[method] = callback

    def reg_hub_notify_callback(self, method:str, callback:Callable[[bytes],None]):
        self.hub_notify_callback[method] = callback

    def call_hub_request(self, method:str, argvs:bytes) -> int:
        from app import app
        msg_cb_id = self.request_msg_cb_id
        self.request_msg_cb_id += 1
        app().ctx.call_rpc(self.entity_id, msg_cb_id, method, argvs)
        return msg_cb_id
    
    def reg_hub_callback(self, msg_cb_id:int, rsp:callback):
        self.hub_callback[msg_cb_id] = rsp

    def call_hub_response(self, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.call_rsp(self.entity_id, self.entity_id, msg_cb_id, argvs)

    def call_hub_response_error(self, msg_cb_id:int, argvs:bytes):
        from app import app
        app().ctx.call_err(self.entity_id, msg_cb_id, argvs)

    def call_hub_notify(self, method:str, argvs:bytes):
        from app import app
        app().ctx.call_ntf(self.entity_id, method, argvs)

class player_manager(object):
    def __init__(self):
        self.players:dict[str, player] = {}
        
    def add_player(self, _player:player):
        self.players[_player.entity_id] = _player
        
    def update_player(self, entity_id:str, argvs: dict):
        _player = self.get_player(entity_id)
        _player.update_player(argvs)

    def get_player(self, entity_id:str) -> player:
        if entity_id in self.players:
            return self.players[entity_id]
        return None
    
    def del_player(self, entity_id:str):
        if entity_id in self.players:
            del self.players[entity_id]