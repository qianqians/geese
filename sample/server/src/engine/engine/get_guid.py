# -*- coding: UTF-8 -*-
from collections.abc import Callable
from .base_dbproxy_handle import base_dbproxy_handle

class get_guid(base_dbproxy_handle):
    def __init__(self, db:str, collection:str) -> None:
        base_dbproxy_handle.__init__(self)
        self.__db__ = db
        self.__collection__ = collection

    def gen(self, callback:Callable[[int],None]) -> int:
        print("get_guid gen begin!")
        from app import app
        while not self.__get_dbproxy__().get_guid(self.__db__, self.__collection__, lambda guid: callback(guid)):
            app().ctx.log("error", "gen guid exception dbproxy:{} __db__:{} __collection__:{}".format(self.__dbproxy__, self.__db__, self.__collection__))
            self.__random_new_dbproxy__()
        print("get_guid gen end!")