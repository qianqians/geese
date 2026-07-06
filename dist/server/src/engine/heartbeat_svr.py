from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class heartbeat_call_heartbeat_cb(object):
    def __init__(self, _cb_uuid:int, _entity:player):
        self.entity = _entity
        self.cb:Callable[[client_time_info], None] = None
        self.err:Callable[[error_code], None] = None
        self.rsp = callback(lambda: self.entity.del_client_callback(_cb_uuid))
        self.entity.reg_client_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        _time_info = protcol_to_client_time_info(inArray[0])
        self.cb(_time_info)

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]
        self.err(_err)

    def callBack(self, _cb:Callable[[client_time_info], None], _err:Callable[[error_code], None]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class heartbeat_caller(object):
    def __init__(self, entity:player):
        self.entity = entity

    def call_heartbeat(self, entity_id:str):
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f = []
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f.append(entity_id)
        _cb_uuid = self.entity.call_client_request("call_heartbeat", dumps(_argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f))

        return heartbeat_call_heartbeat_cb(_cb_uuid, self.entity)



