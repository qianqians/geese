# -*- coding: UTF-8 -*-
from __future__ import annotations
from abc import ABC, abstractmethod
from collections.abc import Callable
from threading import Timer

from engine.engine.app import app

from .base_dbproxy_handle import base_dbproxy_handle
from .dbproxy import DBExtensionError

def SaveDBDescribe(db:str, collection:str):
    def wrapper(cls):
        cls.__db__ = db
        cls.__collection__ = collection
        return cls
    return wrapper

class save(ABC, base_dbproxy_handle):
    def __init__(self, entity_id:str) -> None:
        ABC.__init__(self)
        base_dbproxy_handle.__init__(self)

        self.entity_id = entity_id
        self.__is_dirty__ = False
        self.__save_timer__ = None

        from .app import app
        app().save_mgr.add_save_entity(self)

    def set_dirty(self):
        self.__is_dirty__ = True
        if self.__save_timer__ == None:
            from .app import app
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
        from .app import app
        app().trace(f"save_entity entity_id:{self.entity_id} data:{data}")
        result = self.__get_dbproxy__().updata_object(self.__db__, self.__collection__, self.__query__, data, True,
            lambda result : self.__updata_object_callback__(result))
        if not result:
            self.__updata_object_callback__(result)

    async def load_or_create_entity(query:dict, db:str, collection:str, creator:Callable[[], dict], callback:Callable[[dict], None]):
        from .app import app
        try:
            __dbproxy__ = app().dbproxy_mgr.get_dbproxy()
            data = await __dbproxy__.get_object_one(db, collection, query)
            app().trace(f"load_or_create_entity db:{db} collection:{collection} query:{query} data:{data}")
            if data == None:
                data = creator()
            callback(data)
        except Exception as err:
            app().error(f"save load_or_create_entity exception err:{err}")

    @abstractmethod
    def store(self) -> dict:
        pass

class save_manager(object):
    def __init__(self):
        self.saves:dict[str, save] = {}

    def add_save_entity(self, obj:save):
        self.saves[obj.entity_id] = obj

    def del_save_entity(self, entity_id:str):
        del self.saves[entity_id]

    def for_each_entity(self, callback:Callable[[save]]):
        for entity in self.saves.values():
            callback(entity)
