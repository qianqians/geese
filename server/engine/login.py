# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
from .base_dbproxy_handle import base_dbproxy_handle

class login_event_handle(ABC, base_dbproxy_handle):
    def __init__(self, db:str, collection:str):
        ABC.__init__(self)
        base_dbproxy_handle.__init__(self)
        
        self.__db__ = db
        self.__collection__ = collection
        
        self.kick_off_client_callback:dict[str, Callable[[bool],None]] = {}
    
    @abstractmethod
    async def on_login(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, argvs:dict):
        pass
    
    @abstractmethod
    async def on_reconnect(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str):
        pass
    
    def on_entry_entity(self, entity_id:str, is_main:bool, is_reconnect:bool, new_gate_name:str, new_conn_id:str):
        from app import app
        is_entry_player = app().player_mgr.update_player_conn(entity_id, is_main, is_reconnect, new_gate_name, new_conn_id) 
        is_entry_entity = app().entity_mgr.update_entity_conn(entity_id, is_reconnect, new_gate_name, new_conn_id)
        if (not is_entry_player) and (not is_entry_entity) and is_reconnect:
            app().ctx.hub_call_client_delete_remote_entity(new_gate_name, entity_id)
    
    def __replace_client__(self, old_gate_name:str, old_conn_id:str, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str, is_replace:bool, prompt_info:str):
        from app import app
        app().ctx.hub_call_replace_client(old_gate_name, old_conn_id, new_gate_name, new_conn_id, sdk_uuid, token, is_replace, prompt_info)

class login_service(object):
    def __init__(self, login_event_handle:login_event_handle) -> None:
        self.__login_event_handle__ = login_event_handle

    async def login(self, gate_name:str, conn_id:str, sdk_uuid:str, argvs:dict):
        await self.__login_event_handle__.on_login(gate_name, conn_id, sdk_uuid, argvs)
        
    async def reconnect(self, gate_name:str, conn_id:str, sdk_uuid:str, token:str):
        await self.__login_event_handle__.on_reconnect(gate_name, conn_id, sdk_uuid, token)
    
    def on_transfer_entity_control(self, entity_id:str, is_main: bool, is_reconnect:bool, new_gate_name:str, new_conn_id:str):
        self.__login_event_handle__.on_entry_entity(entity_id, is_main, is_reconnect, new_gate_name, new_conn_id)