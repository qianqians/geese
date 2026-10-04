from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

class em_platform(IntFlag):
    EPlatformGoogle = 1
    EPlatformIphone = 2
    EPlatformSteam = 3
    EPlatformWXMiniGame = 4


#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class login_create_character_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_b09e2393_3876_3f7c_be40_cdd789a21e87 = []
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_b09e2393_3876_3f7c_be40_cdd789a21e87))

    def err(self, errCode:int):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_b09e2393_3876_3f7c_be40_cdd789a21e87 = []
        _argv_b09e2393_3876_3f7c_be40_cdd789a21e87.append(errCode)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_b09e2393_3876_3f7c_be40_cdd789a21e87))

class login_select_character_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_6f89916d_93ec_3d09_b91d_511264985bc0 = []
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_6f89916d_93ec_3d09_b91d_511264985bc0))

    def err(self, errCode:int):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_6f89916d_93ec_3d09_b91d_511264985bc0 = []
        _argv_6f89916d_93ec_3d09_b91d_511264985bc0.append(errCode)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_6f89916d_93ec_3d09_b91d_511264985bc0))

class login_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_create_character:list[Callable[[login_create_character_rsp, str, em_role_gender, str, str], None]] = []
        self.entity.reg_client_request_callback("create_character", self.create_character)
        self.on_select_character:list[Callable[[login_select_character_rsp, str], None]] = []
        self.entity.reg_client_request_callback("select_character", self.select_character)

    def create_character(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _player_nick_name = inArray[0]
        _gender = inArray[1]
        _appearance = inArray[2]
        _scene = inArray[3]
        rsp = login_create_character_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_create_character:
            fn(rsp, _player_nick_name, _gender, _appearance, _scene)

    def select_character(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _player_id = inArray[0]
        rsp = login_select_character_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_select_character:
            fn(rsp, _player_id)




