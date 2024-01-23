from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class login_login_cb(object):
    def __init__(self, _cb_uuid:int, _entity:subentity|player):
        self.entity = _entity
        self.cb:Callable[[bool]] = None
        self.err:Callable[[error_code]] = None
        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))
        self.entity.reg_hub_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        _is_displace = inArray[0]
        self.cb(_is_displace)

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]
        self.err(_err)

    def callBack(self, _cb:Callable[[bool]], _err:Callable[[error_code]]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class login_caller(object):
    def __init__(self, entity:subentity|player):
        self.entity = entity

    def login(self, sdk_uuid:str):
        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1 = []
        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1.append(sdk_uuid)
        _cb_uuid = self.entity.call_hub_request("login", dumps(_argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1))

        return login_login_cb(_cb_uuid, self.entity)



