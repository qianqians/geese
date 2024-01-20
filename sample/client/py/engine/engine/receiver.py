# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable

from .base_entity import base_entity

class receiver(ABC, base_entity):
    def __init__(self, entity_type:str, entity_id:str) -> None:
        base_entity.__init__(entity_type, entity_id)

        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}

        from app import app
        app().receiver_mgr.add_receiver(self)

    @abstractmethod
    def update_receiver(self, argvs: dict):
        pass

    def handle_hub_notify(self, method:str, hub_name:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
        if _call_handle != None:
            _call_handle(hub_name, argvs)
        else:
            self.error("unhandle notify method:{}".format(method))

    def reg_hub_notify_callback(self, method:str, callback:Callable[[bytes],None]):
        self.hub_notify_callback[method] = callback

class receiver_manager(object):
    def __init__(self):
        self.receivers:dict[str, receiver] = {}
        
    def add_receiver(self, _receiver:receiver):
        self.receivers[_receiver.entity_id] = _receiver

    def update_receiver(self, entity_id:str, argvs: dict):
        _receiver = self.get_receiver(entity_id)
        _receiver.update_receiver(argvs)

    def get_receiver(self, entity_id) -> receiver:
        return self.receivers[entity_id]
    
    def del_receiver(self, entity_id):
        del self.receivers[entity_id]