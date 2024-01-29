# -*- coding: UTF-8 -*-
from collections.abc import Callable
import random

from .base_entity import base_entity
from .callback import callback

class subentity(base_entity):
    def __init__(self, source_hub_name:str, entity_type:str, entity_id:str) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.request_msg_cb_id = random.randint(100, 10011)
        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}

        self.hub_callback:dict[int, callback] = {}

        self.source_hub_name = source_hub_name

        from app import app
        app().subentity_mgr.add_subentity(self)

    def del_callback(self, msg_cb_id:int) -> bool:
        if msg_cb_id not in self.hub_callback:
            return False
        del self.hub_callback[msg_cb_id]
        return True

    def handle_hub_response(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle._callback(argvs)
            del self.hub_callback[msg_cb_id]
        else:
            self.error("unhandle response msg_cb_id:{}, hub:{}".format(msg_cb_id, hub_name))

    def handle_hub_response_error(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle.error(argvs)
            del self.hub_callback[msg_cb_id]
        else:
            self.error("unhandle response msg_cb_id:{}, hub:{}".format(msg_cb_id, hub_name))

    def handle_hub_notify(self, source_hub:str, method:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, source_hub))

    def reg_hub_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.hub_notify_callback[method] = callback

    def call_hub_request(self, method:str, argvs:bytes) -> int:
        from app import app
        msg_cb_id = self.request_msg_cb_id
        self.request_msg_cb_id += 1
        app().ctx.hub_call_hub_rpc(self.source_hub_name, self.entity_id, msg_cb_id, method, argvs)
        return msg_cb_id
        
    def reg_hub_callback(self, msg_cb_id:int, rsp:callback):
        self.hub_callback[msg_cb_id] = rsp

    def call_hub_notify(self, method:str, argvs:bytes):
        from app import app
        app().ctx.hub_call_hub_ntf(self.source_hub_name, method, argvs)

class subentity_manager(object):
    def __init__(self):
        self.subentities:dict[str, subentity] = {}
        
    def add_subentity(self, _entity:subentity):
        self.subentities[_entity.entity_id] = _entity

    def get_subentity(self, entity_id) -> subentity:
        if entity_id in self.subentities:
            return self.subentities[entity_id]
        return None
    
    def del_subentity(self, entity_id):
        if entity_id in self.subentities:
            del self.subentities[entity_id]