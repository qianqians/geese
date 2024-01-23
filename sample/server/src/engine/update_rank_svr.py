from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class update_rank_call_update_rank_cb(object):
    def __init__(self, _cb_uuid:int, _entity:subentity):
        self.entity = _entity
        self.cb:Callable[[], None] = None
        self.err:Callable[[error_code], None] = None
        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))
        self.entity.reg_hub_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        self.cb()

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]

        self.err(_err)

    def callBack(self, _cb:Callable[[], None], _err:Callable[[error_code], None]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class update_rank_caller(object):
    def __init__(self, entity:subentity):
        self.entity = entity

    def call_update_rank(self, entity_id:str):
        _argv_00ceffc5_7c14_306e_af59_48f2ba7a8702 = []
        _argv_00ceffc5_7c14_306e_af59_48f2ba7a8702.append(entity_id)
        _cb_uuid = self.entity.call_hub_request("call_update_rank", dumps(_argv_00ceffc5_7c14_306e_af59_48f2ba7a8702))

        return update_rank_call_update_rank_cb(_cb_uuid, self)

#this module code is codegen by geese codegen for python
class update_rank_call_update_rank_rsp(session):
    def __init__(self, source:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, source)
        self.entity = entity
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, ):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_00ceffc5_7c14_306e_af59_48f2ba7a8702 = []
        self.entity.call_hub_response(self.source, self.msg_cb_id, dumps(_argv_00ceffc5_7c14_306e_af59_48f2ba7a8702))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_00ceffc5_7c14_306e_af59_48f2ba7a8702 = [self.uuid_0d040228_1392_3e8e_b7fd_548fb8073ed7]
        _argv_00ceffc5_7c14_306e_af59_48f2ba7a8702.append(err)
        self.entity.call_hub_response_error(self.source, self.msg_cb_id, dumps(_argv_00ceffc5_7c14_306e_af59_48f2ba7a8702))

class update_rank_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_call_update_rank:list[Callable[[update_rank_call_update_rank_rsp, str], None]] = []
        self.entity.reg_hub_request_callback("call_update_rank", self.call_update_rank)

    def call_update_rank(self, source:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _entity_id = inArray[1]
        rsp = update_rank_call_update_rank_rsp(source, msg_cb_id)
        for fn in self.on_call_update_rank:
            fn(rsp, _entity_id)


