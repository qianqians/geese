# -*- coding: UTF-8 -*-
from collections.abc import Callable
import asyncio
from .base_dbproxy_handle import base_dbproxy_handle

async def __gen_guid_set_future__(future:asyncio.Future, guid:int):
    future.set_result(guid)

class get_guid(base_dbproxy_handle):
    def __init__(self, db:str, collection:str) -> None:
        base_dbproxy_handle.__init__(self)
        self.__db__ = db
        self.__collection__ = collection

    def __gen_callback__(self, guid:int, future:asyncio.Future):
        from app import app
        app().run_coroutine_async(__gen_guid_set_future__(future, guid))

    async def gen(self) -> int:
        print("get_guid gen begin!")
        future = asyncio.Future()
        from app import app
        while not self.__get_dbproxy__().get_guid(self.__db__, self.__collection__, lambda guid: self.__gen_callback__(guid)):
            app().ctx.log("error", "gen guid exception dbproxy:{} __db__:{} __collection__:{}".format(self.__dbproxy__, self.__db__, self.__collection__))
            self.__random_new_dbproxy__()
        print("get_guid gen end!")
        return await future