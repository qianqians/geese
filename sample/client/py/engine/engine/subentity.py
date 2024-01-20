# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
import random

from .base_entity import base_entity
from .callback import callback

class subentity(ABC, base_entity):
    def __init__(self, entity_type:str, entity_id:str) -> None:
        base_entity.__init__(entity_type, entity_id)

        self.request_msg_cb_id = random.randint(100, 10011)
        self.hub_notify_callback:dict[str:Callable[[bytes],None]] = {}

        self.hub_callback:dict[int, callback] = {}

        from app import app
        app().subentity_mgr.add_subentity(self)

    @abstractmethod
    def update_subentity(self, argvs: dict):
        pass

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

    def call_hub_notify(self, method:str, argvs:bytes):
        from app import app
        app().ctx.call_ntf(self.source_hub_name, method, argvs)

class subentity_manager(object):
    def __init__(self):
        self.subentities:dict[str, subentity] = {}
        
    def add_subentity(self, _entity:subentity):
        self.subentities[_entity.entity_id] = _entity

    def update_subentity(self, entity_id:str, argvs: dict):
        _subentity = self.get_subentity(entity_id)
        _subentity.update_subentity(argvs)

    def get_subentity(self, entity_id) -> subentity:
        return self.subentities[entity_id]
    
    def del_subentity(self, entity_id):
        del self.subentities[entity_id]