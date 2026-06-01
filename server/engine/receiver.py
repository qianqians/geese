# -*- coding: UTF-8 -*-
from collections.abc import Callable
from .base_entity import base_entity

class receiver(base_entity):
    def __init__(self, source_hub_name:str, entity_type:str, entity_id:str) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}
        self.source_hub_name = source_hub_name

        from app import app
        app().receiver_mgr.add_receiver(self)

    def handle_hub_notify(self, source_hub:str, method:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, source_hub))

    def reg_hub_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.hub_notify_callback[method] = callback

class receiver_manager(object):
    def __init__(self):
        self.receivers:dict[str, receiver] = {}
        
    def add_receiver(self, _receiver:receiver):
        self.receivers[_receiver.entity_id] = _receiver

    def get_receiver(self, entity_id) -> receiver:
        if entity_id in self.receivers:
            return self.receivers[entity_id]
        return None
    
    def del_receiver(self, entity_id):
        if entity_id in self.receivers:
            del self.receivers[entity_id]