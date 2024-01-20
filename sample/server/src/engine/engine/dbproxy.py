# -*- coding: UTF-8 -*-
from collections.abc import Callable
import uuid
import asyncio
from .bson import *

from .context import context
from .dbproxy_msg_handle import dbproxy_msg_handle

class DBExtensionError(Exception):
    def __init__(self, db:str, collection:str, operate:str) -> None:
        self.db = db
        self.collection = collection
        self.operate = operate

async def __get_object_one_callback_set_future__(future:asyncio.Future, data:dict):
    future.set_result(data)

async def __get_object_one_callback_set_future_error__(future:asyncio.Future, err):
    future.set_exception(err)

class dbproxy(object):
    def __init__(self, dbproxy_name:str, ctx:context, handle:dbproxy_msg_handle):
        self.__dbproxy_name__ = dbproxy_name
        
        self.__ctx__ = ctx
        self.__handle__ = handle
        
    def get_guid(self, db:str, collection:str, callback:Callable[[int],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_get_guid_callback(callback_id, callback)
        return self.__ctx__.get_guid(self.__dbproxy_name__, db, collection, callback_id)
        
    def create_object(self, db:str, collection:str, obj:dict, callback:Callable[[bool],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_create_object_callback(callback_id, callback)
        return self.__ctx__.create_object(self.__dbproxy_name__, db, collection, callback_id, encode(obj))
        
    def updata_object(self, db:str, collection:str, query:dict, update:dict, upsert:bool, callback:Callable[[bool],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_updata_object_callback(callback_id, callback)
        return self.__ctx__.update_object(self.__dbproxy_name__, db, collection, callback_id, encode(query), encode(update), upsert)
        
    def find_and_modify(self, db:str, collection:str, query:dict, update:dict, new:bool, upsert:bool, callback:Callable[[dict],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_find_and_modify_callback(callback_id, callback)
        return self.__ctx__.find_and_modify(self.__dbproxy_name__, db, collection, callback_id, encode(query), encode(update), new, upsert)
        
    def remove_object(self, db:str, collection:str, query:dict, callback:Callable[[bool],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_remove_object_callback(callback_id, callback)
        return self.__ctx__.remove_object(self.__dbproxy_name__, db, collection, callback_id, encode(query))
        
    def get_object_count(self, db:str, collection:str, query:dict, callback:Callable[[int],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_get_object_count_callback(callback_id, callback)
        return self.__ctx__.get_object_count(self.__dbproxy_name__, db, collection, callback_id, encode(query))
        
    def get_object_info(self, db:str, collection:str, query:dict, skip:int, limit:int, sort:str, ascending:bool, callback:Callable[[list],None], end_callback:Callable[[],None]):
        callback_id = str(uuid.uuid4())
        self.__handle__.reg_get_object_info_callback(callback_id, callback)
        self.__handle__.reg_get_object_info_end_callback(callback_id, end_callback)
        return self.__ctx__.get_object_info(self.__dbproxy_name__, db, collection, callback_id, encode(query), skip, limit, sort, ascending)

    def get_object_info_simple(self, db:str, collection:str, query:dict, callback:Callable[[list],None], end_callback:Callable[[],None]):
        return self.get_object_info(db, collection, query, 0, 100, "", False, callback, end_callback)
    
    def __get_object_one_callback_data__(data_list:list, db:str, collection:str, future:asyncio.Future):
        print(f"__get_object_one_callback_data__ data_list:{data_list}")
        from app import app
        if len(data_list) == 1:
            app().run_coroutine_async(__get_object_one_callback_set_future__(future, data_list[0]))
        elif len(data_list) == 0:
            app().run_coroutine_async(__get_object_one_callback_set_future__(future, None))
        else:
            app().run_coroutine_async(__get_object_one_callback_set_future_error__(future, DBExtensionError(db, collection, "db error more then one object")))
    
    async def get_object_one(self, db:str, collection:str, query:dict) -> dict:
        future = asyncio.Future()
        self.get_object_info(db, collection, query, 0, 100, "", False, 
            lambda _list: dbproxy.__get_object_one_callback_data__(_list, db, collection, future),
            lambda : print("get_object_one end!"))
        return await future
        
class dbproxy_manager(object):
    def __init__(self, ctx:context, handle:dbproxy_msg_handle):
        self.__ctx__ = ctx
        self.__handle__ = handle
        
        self.__dbproxies__:dict[str, dbproxy] = {}
    
    def get_dbproxy(self):
        dbproxy_name = self.__ctx__.entry_dbproxy_service()
        if dbproxy_name not in self.__dbproxies__:
            proxy = dbproxy(dbproxy_name, self.__ctx__, self.__handle__)
            self.__dbproxies__[dbproxy_name] = proxy
        proxy = self.__dbproxies__[dbproxy_name]
        return proxy