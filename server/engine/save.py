# -*- coding: UTF-8 -*-
from __future__ import annotations
from abc import ABC, abstractmethod
from collections.abc import Callable
from threading import Timer

from .base_dbproxy_handle import base_dbproxy_handle
from .dbproxy import DBExtensionError

def SaveDBDescribe(db:str, collection:str):
    def wrapper(cls):
        cls.__db__ = db
        cls.__collection__ = collection
        return cls
    return wrapper

class save(ABC, base_dbproxy_handle):
    def __init__(self) -> None:
        ABC.__init__(self)
        base_dbproxy_handle.__init__(self)
        
        self.__is_dirty__ = False
        self.__save_timer__ = None

        from app import app
        app().save_mgr.add_save_entity(self)

    def set_dirty(self):
        self.__is_dirty__ = True
        if self.__save_timer__ == None:
            from app import app
            self.__save_timer__ = Timer(app().ctx.save_time_interval(), self.save_entity)
            self.__save_timer__.start()

    def __updata_object_callback__(self, result:bool):
        if result:
            self.__is_dirty__ = False
        else:
            self.__random_new_dbproxy__()
            self.save_entity()

    def save_entity(self):
        if not self.__is_dirty__:
            return
        
        self.__save_timer__ = None
        
        data = self.store()
        result = self.__get_dbproxy__().updata_object(self.__db__, self.__collection__, self.__query__, data, False,
            lambda result : self.__updata_object_callback__(result))
        if not result:
            self.__updata_object_callback__(result)

    def __creator_entity_callback__(self, result:bool, data):
        if not result:
            self.__random_new_dbproxy__()
            result = self.__get_dbproxy__().create_object(self.__db__, self.__collection__, data, 
                lambda result : self.__creator_entity_callback__(result))
            if not result:
                self.__random_new_dbproxy__()
                self.__creator_entity_callback__(result)

    async def load_or_create_entity(query:dict, callback:Callable[[dict], None]):
        while True:
            try:
                _new_obj = save()
                data = await _new_obj.__get_dbproxy__().get_object_one(_new_obj.__db__, _new_obj.__collection__, query)
                if data == None:
                    data = save.create()
                    _new_obj.__query__ = query
                    result = _new_obj.__get_dbproxy__().create_object(_new_obj.__db__, _new_obj.__collection__, data, 
                        lambda result : _new_obj.__creator_entity_callback__(result))
                    if not result:
                        _new_obj.__creator_entity_callback__(result)
                callback(data)
            except Exception as err:
                from app import app
                app().error("save load_or_create_entity exception dbproxy:{} __db__:{} __collection__:{}".format(
                    _new_obj.__dbproxy__, _new_obj.__db__, _new_obj.__collection__))
                _new_obj.__random_new_dbproxy__()

    @staticmethod
    @abstractmethod
    def create() -> dict:
        pass

    @staticmethod
    @abstractmethod
    def load(self, data:dict) -> save:
        pass

    @abstractmethod
    def store(self) -> dict:
        pass
    
class save_manager(object):
    def __init__(self):
        self.saves:dict[str, save] = {}
        
    def add_save_entity(self, obj:save):
        self.saves[obj.__entity_id__] = obj
        
    def del_save_entity(self, entity_id:str):
        del self.saves[entity_id]
        
    def for_each_entity(self, callback:Callable[[save]]):
        for entity in self.saves.values():
            callback(entity)