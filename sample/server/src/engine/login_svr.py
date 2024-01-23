from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class login_login_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, is_displace:bool):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1 = []
        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1.append(is_displace)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1 = [self.uuid_4ca8d1d2_e4d6_3ac0_875b_d35c29f54aad]
        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1))

class login_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_login:list[Callable[[login_login_rsp, str], None]] = []
        self.entity.reg_client_request_callback("login", self.login)

    def login(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _sdk_uuid = inArray[0]
        rsp = login_login_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_login:
            fn(rsp, _sdk_uuid)




