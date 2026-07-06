# -*- coding: UTF-8 -*-
from collections.abc import Callable
from .bson import *

class dbproxy_msg_handle(object):
    def __init__(self) -> None:
        self.__get_guid_callback__:dict[str, Callable[[int],None]] = {}
        self.__create_object_callback__:dict[str, Callable[[bool],None]] = {}
        self.__updata_object_callback__:dict[str, Callable[[bool],None]] = {}
        self.__find_and_modify_callback__:dict[str, Callable[[dict],None]] = {}
        self.__remove_object_callback__:dict[str, Callable[[bool],None]] = {}
        self.__get_object_count_callback__:dict[str, Callable[[int],None]] = {}
        self.__get_object_info_callback__:dict[str, Callable[[list],None]] = {}
        self.__get_object_info_end_callback__:dict[str, Callable[[],None]] = {}
        
    def reg_get_guid_callback(self, callback_id:str, callback:Callable[[int],None]):
        self.__get_guid_callback__[callback_id] = callback
        
    def reg_create_object_callback(self, callback_id:str, callback:Callable[[bool],None]):
        self.__create_object_callback__[callback_id] = callback
        
    def reg_updata_object_callback(self, callback_id:str, callback:Callable[[bool],None]):
        self.__updata_object_callback__[callback_id] = callback
        
    def reg_find_and_modify_callback(self, callback_id:str, callback:Callable[[dict],None]):
        self.__find_and_modify_callback__[callback_id] = callback
        
    def reg_remove_object_callback(self, callback_id:str, callback:Callable[[bool],None]):
        self.__remove_object_callback__[callback_id] = callback
        
    def reg_get_object_count_callback(self, callback_id:str, callback:Callable[[int],None]):
        self.__get_object_count_callback__[callback_id] = callback
        
    def reg_get_object_info_callback(self, callback_id:str, callback:Callable[[list],None]):
        self.__get_object_info_callback__[callback_id] = callback
        
    def reg_get_object_info_end_callback(self, callback_id:str, callback:Callable[[],None]):
        self.__get_object_info_end_callback__[callback_id] = callback
        
    def on_ack_get_guid(self, callback_id:str, guid:int):
        cb = self.__get_guid_callback__[callback_id]
        cb(guid)
        del self.__get_guid_callback__[callback_id]
        
    def on_ack_create_object(self, callback_id:str, result:bool):
        cb = self.__create_object_callback__[callback_id]
        cb(result)
        del self.__create_object_callback__[callback_id]
        
    def on_ack_updata_object(self, callback_id:str, result:bool):
        cb = self.__updata_object_callback__[callback_id]
        cb(result)
        del self.__updata_object_callback__[callback_id]
        
    def on_ack_find_and_modify(self, callback_id:str, obj:bytes):
        cb = self.__find_and_modify_callback__[callback_id]
        cb(decode(obj))
        del self.__find_and_modify_callback__[callback_id]
        
    def on_ack_remove_object(self, callback_id:str, result:bool):
        cb = self.__remove_object_callback__[callback_id]
        cb(result)
        del self.__remove_object_callback__[callback_id]
        
    def on_ack_get_object_count(self, callback_id:str, count:int):
        cb = self.__get_object_count_callback__[callback_id]
        cb(count)
        del self.__get_object_count_callback__[callback_id]
    
    def on_ack_get_object_info(self, callback_id:str, objs:bytes):
        cb = self.__get_object_info_callback__[callback_id]
        obj_doc = decode(objs)
        cb(obj_doc["__list__"])
        del self.__get_object_info_callback__[callback_id]
        
    def on_ack_get_object_info_end(self, callback_id:str):
        cb = self.__get_object_info_end_callback__[callback_id]
        cb()
        del self.__get_object_info_end_callback__[callback_id]
        

