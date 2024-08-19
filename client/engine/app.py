# -*- coding: UTF-8 -*-
from __future__ import annotations
import time
from threading import Timer
import _thread

from collections.abc import Callable
import asyncio
from .msgpack import *

from .pyclient import ClientPump
from .context import context
from .conn_msg_handle import conn_msg_handle
from .player import *
from .subentity import *
from .receiver import *

def __handle_poll_coroutine_thread__(_app:app):
    _app.poll_coroutine_thread()

class client_event_handle(ABC):
    @abstractmethod
    def on_kick_off(self, prompt_info:str):
        pass

    @abstractmethod
    def on_transfer_complete(self):
        pass

def singleton(cls):
    _instance = {}

    def inner():
        if cls not in _instance:
            _instance[cls] = cls()
        return _instance[cls]
    return inner
    
@singleton
class app(object):
    def __init__(self):
        self.ctx:context = None
        
        self.__is_run__ = True
        self.__conn_handle__:conn_msg_handle = None
        self.__entity_create_method__:dict[str, Callable[[str, dict]]] = {}
        self.__conn_id__:str = None
        self.__conn_id_callback__:Callable[[str], None] = None
        self.__pump__ = None
        
        self.player_mgr:player_manager = None
        self.subentity_mgr:subentity_manager = None
        self.receiver_mgr:receiver_manager = None

        self.__hub_global_callback__:dict[str, Callable[[str, bytes]]] = {}
        self.__loop__ = asyncio.new_event_loop()
        
        self.client_event_handle = None
    
    def build(self, handle:client_event_handle):
        self.ctx = context()
        self.client_event_handle = handle
        self.__conn_handle__ = conn_msg_handle()
        self.__pump__ = ClientPump(self.ctx.ctx)
        
        self.player_mgr = player_manager()
        self.subentity_mgr = subentity_manager()
        self.receiver_mgr = receiver_manager()
        
        self.heartbeats()
        return self
    
    def heartbeats(self):
        tick = Timer(3, self.heartbeats)
        tick.start()
        return self.ctx.heartbeats()
    
    def on_kick_off(self, prompt_info:str):
        self.client_event_handle.on_kick_off(prompt_info)

    def on_transfer_complete(self):
        self.client_event_handle.on_transfer_complete()

    def on_call_global(self, method:str, hub_name:str, argvs:bytes):
        _call_handle = self.__hub_global_callback__[method]
        if _call_handle != None:
            _call_handle(hub_name, argvs)
        else:
            print("unhandle global method:{}".format(method))

    def register_global_method(self, method:str, callback:Callable[[str, bytes]]):
        self.__hub_global_callback__[method] = callback
    
    def connect_tcp(self, addr:str, port:int, callback:Callable[[str], None]) -> bool:
        print("connect_tcp begin!")
        self.__conn_id_callback__ = callback
        return self.ctx.connect_tcp(addr, port)
    
    def connect_ws(self, host:str, callback:Callable[[str], None]) -> bool:
        print("connect_ws begin!")
        self.__conn_id_callback__ = callback
        return self.ctx.connect_ws(host)
    
    def login(self, sdk_uuid:str, argvs:dict) -> bool:
        return self.ctx.login(sdk_uuid, dumps(argvs))
    
    def reconnect(self, account_id:str, token:str) -> bool:
        return self.ctx.reconnect(account_id, token)
    
    def request_hub_service(self, service_name:str) -> bool:
        return self.ctx.request_hub_service(service_name)

    def register(self, entity_type:str, creator:Callable[[str, dict]]):
        self.__entity_create_method__[entity_type] = creator
        return self
        
    def create_entity(self, entity_type:str, entity_id:str, argvs: dict):
        _creator = self.__entity_create_method__[entity_type]
        _creator(entity_id, argvs)
        
    def update_entity(self, entity_type:str, entity_id:str, argvs: dict):
        self.player_mgr.update_player(entity_id, argvs)
        self.subentity_mgr.update_subentity(entity_id, argvs)
        self.receiver_mgr.update_receiver(entity_id, argvs)

    def delete_entity(self, entity_id:str):
        self.player_mgr.del_player(entity_id)
        self.subentity_mgr.del_subentity(entity_id)
        self.receiver_mgr.del_receiver(entity_id)
    
    def run_coroutine_async(self, coro):
        asyncio.run_coroutine_threadsafe(coro, self.__loop__)
        
    def close(self):
        self.__is_run__ = False
            
    def poll_coroutine_thread(self):
        asyncio.set_event_loop(self.__loop__)
        self.__loop__.run_forever()

    def poll_conn_msg(self):
        while True:
            if not self.__pump__.poll_conn_msg(self.__conn_handle__):
                break
    
    def poll(self):
        while self.__is_run__:
            start = time.time()
            self.poll_conn_msg()
            tick = time.time() - start
            if tick < 0.033:
                time.sleep(0.033 - tick)
            
    def run(self):
        _thread.start_new_thread(__handle_poll_coroutine_thread__, (self,))
        self.poll()
    