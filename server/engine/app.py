# -*- coding: UTF-8 -*-
from __future__ import annotations
import os
import sys
sys.path.append(os.path.dirname(os.path.realpath(__file__)))
import time
import _thread

from collections.abc import Callable
import json
import uuid
import signal
import asyncio

from .redis import *

from .pyhub import HubConnMsgPump, HubDBMsgPump
from .context import context
from .dbproxy_msg_handle import dbproxy_msg_handle
from .conn_msg_handle import conn_msg_handle
from .dbproxy import *
from .service import *
from .save import *
from .player import *
from .entity import *
from .subentity import *
from .receiver import *
from .login import *
from .get_guid import *
from .dbproxy import *

def __handle_exception__(exc_type, exc_value, tb):
    app().error("error Uncaught exception:{}, exc_value:{}, tb:{}".format(exc_type, exc_value, tb))
sys.excepthook = __handle_exception__

def __handle_sigterm__(signal, frame):
    app().close()

def __handle_poll_db_msg_thread__(_app:app):
    _app.poll_db_msg_thread()

def __handle_poll_coroutine_thread__(_app:app):
    _app.poll_coroutine_thread()

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
        self.__dbproxy_handle__:dbproxy_msg_handle = None
        self.__conn_handle__:conn_msg_handle = None
        self.__entity_create_method__:dict[str, Callable[[str, str, dict]]] = {}
        self.__loop__ = None
        self.__conn_pump__ = None
        self.__db_pump__ = None
        
        self.config:dict = None
        self.redis_proxy:Redis = None
        self.login_handle:login_service = None
        self.dbproxy_mgr:dbproxy_manager = None
        self.service_mgr:service_manager = None
        self.save_mgr:save_manager = None
        self.player_mgr:player_manager = None
        self.entity_mgr:entity_manager = None
        self.subentity_mgr:subentity_manager = None
        self.receiver_mgr:receiver_manager = None
        
        self.__loop__ = asyncio.new_event_loop()
        
        signal.signal(signal.SIGTERM, __handle_sigterm__)
        
    def build(self, cfg_file:str):
        self.config = json.load(open(cfg_file))

        self.ctx = context(cfg_file)
        self.__dbproxy_handle__ = dbproxy_msg_handle()
        self.__conn_handle__ = conn_msg_handle()
        self.dbproxy_mgr = dbproxy_manager(self.ctx, self.__dbproxy_handle__)
        self.service_mgr = service_manager()
        self.save_mgr = save_manager()
        self.entity_mgr = entity_manager()
        self.subentity_mgr = subentity_manager()
        self.receiver_mgr = receiver_manager()
        
        self.__conn_pump__ = HubConnMsgPump(self.ctx.ctx)
        self.__db_pump__ = HubDBMsgPump(self.ctx.ctx)

        pool = ConnectionPool.from_url(self.config["redis_url"])
        self.redis_proxy = Redis(connection_pool=pool)
        
    def build_login_service(self, login_event_handle:login_event_handle):
        self.login_handle = login_service(login_event_handle)
        
    def build_player_service(self, player_event_handle:player_event_handle):
        self.player_mgr = player_manager(player_event_handle)
        
    def register_service(self, service:str):
        self.ctx.register_service(service)
        
    def register(self, entity_type:str, creator:Callable[[str, str, dict]]):
        self.__entity_create_method__[entity_type] = creator
        return self
        
    def create_entity(self, entity_type:str, source_hub_name:str, entity_id:str, argvs: dict):
        _creator = self.__entity_create_method__[entity_type]
        _creator(source_hub_name, entity_id, argvs)
        
    def kick_off_client(self, gate_name:str, conn_id:str, prompt_info:str):
        self.ctx.hub_call_kick_off_client(gate_name, conn_id, prompt_info)

    def __unlock_distributed_lock__(self, key:str, value:str):
        try:
            value_lock = self.redis_proxy.get(key)
            if value_lock == value:
                self.redis_proxy.delete(key)
        except:
            self.ctx.log("error", "unlock distributed lock faild key:{} value:{}".format(key, value))

    async def distributed_lock(self, key:str, timeout:int) -> Callable[[], None] | None:
        try:
            value = str(uuid.uuid4())
            while not self.redis_proxy.set(key, value, ex=timeout, nx=True):
                asyncio.sleep(0.08)
            return lambda : self.__unlock_distributed_lock__(key, value)
        except:
            self.ctx.log("error", "distributed lock faild key:{}".format(key))
        return None
    
    def run_coroutine_async(self, coro):
        asyncio.run_coroutine_threadsafe(coro, self.__loop__)
    
    def trace(self, format:str, *argv):
        self.ctx.log("trace", "app " + format.format(argv))
        
    def debug(self, format:str, *argv):
        self.ctx.log("debug", "app " + format.format(argv))

    def info(self, format:str, *argv):
        self.ctx.log("info", "app " + format.format(argv))

    def warn(self, format:str, *argv):
        self.ctx.log("warn", "app " + format.format(argv))

    def error(self, format:str, *argv):
        self.ctx.log("error", "app " + format.format(argv))
        
    def close(self):
        self.__is_run__ = False
        
    def poll_db_msg(self):
        while True:
            if not self.__db_pump__.poll_db_msg(self.__dbproxy_handle__):
                break
            
    def poll_conn_msg(self):
        while True:
            if not self.__conn_pump__.poll_conn_msg(self.__conn_handle__):
                break
    
    def poll_db_msg_thread(self):
        while self.__is_run__:
            start = time.time()
            try:
                self.poll_db_msg()
            except Exception as ex:
                self.error("poll_db_msg_thread Exception:{0}", ex)
            tick = time.time() - start
            if tick < 0.033:
                time.sleep(0.033 - tick)

    def poll_coroutine_thread(self):
        asyncio.set_event_loop(self.__loop__)
        self.__loop__.run_forever()

    def poll(self):
        busy_count = 0
        idle_count = 0
        while self.__is_run__:
            start = time.time()
            try:
                self.poll_conn_msg()
            except Exception as ex:
                self.error("poll Exception:{0}", ex)
            tick = time.time() - start
            if tick < 0.033:
                idle_count += 1
                if idle_count > 5:
                    busy_count = 0
                    self.ctx.set_health_state(True)
                time.sleep(0.033 - tick)
            elif tick > 0.1:
                busy_count += 1
                if busy_count > 5:
                    idle_count = 0
                    self.ctx.set_health_state(False)
            
        self.save_mgr.for_each_entity(lambda entt: entt.save_entity())
            
    def run(self):
        _thread.start_new_thread(__handle_poll_db_msg_thread__, (self,))
        _thread.start_new_thread(__handle_poll_coroutine_thread__, (self,))
        self.poll()