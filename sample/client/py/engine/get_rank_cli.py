from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class get_rank_get_self_rank_cb(object):
    def __init__(self, _cb_uuid:int, _entity:subentity|player):
        self.entity = _entity
        self.cb:Callable[[role_rank_info], None] = None
        self.err:Callable[[error_code], None] = None
        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))
        self.entity.reg_hub_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        _rank = protcol_to_role_rank_info(inArray[0])
        self.cb(_rank)

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]
        self.err(_err)

    def callBack(self, _cb:Callable[[role_rank_info], None], _err:Callable[[error_code], None]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class get_rank_get_rank_cb(object):
    def __init__(self, _cb_uuid:int, _entity:subentity|player):
        self.entity = _entity
        self.cb:Callable[[list[role_rank_info]], None] = None
        self.err:Callable[[error_code], None] = None
        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))
        self.entity.reg_hub_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        _rank_list = []
        for v_e249ecbd_d64c_526b_901b_6d4ddee4b75a in inArray[0]:
            _rank_list.append(role_rank_info_to_protcol(v_e249ecbd_d64c_526b_901b_6d4ddee4b75a))
        self.cb(_rank_list)

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]
        self.err(_err)

    def callBack(self, _cb:Callable[[list[role_rank_info]], None], _err:Callable[[error_code], None]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class get_rank_caller(object):
    def __init__(self, entity:subentity|player):
        self.entity = entity

    def get_self_rank(self, entity_id:str):
        _argv_e22ae90d_2428_3197_a8fb_549203f714e0 = []
        _argv_e22ae90d_2428_3197_a8fb_549203f714e0.append(entity_id)
        _cb_uuid = self.entity.call_hub_request("get_self_rank", dumps(_argv_e22ae90d_2428_3197_a8fb_549203f714e0))

        return get_rank_get_self_rank_cb(_cb_uuid, self.entity)

    def get_rank(self, start:int, end:int):
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7 = []
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.append(start)
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.append(end)
        _cb_uuid = self.entity.call_hub_request("get_rank", dumps(_argv_e869f1c8_1f14_384f_aba6_2af2b54335e7))

        return get_rank_get_rank_cb(_cb_uuid, self.entity)



