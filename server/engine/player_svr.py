from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class player_into_scene_rsp(session):
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

        _argv_85dc3aca_241a_3c31_ae2e_37e652224427 = []
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_85dc3aca_241a_3c31_ae2e_37e652224427))

    def err(self, err_code:int):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_85dc3aca_241a_3c31_ae2e_37e652224427 = []
        _argv_85dc3aca_241a_3c31_ae2e_37e652224427.append(err_code)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_85dc3aca_241a_3c31_ae2e_37e652224427))

class player_talk_npc_rsp(session):
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

        _argv_88591603_b9dc_329d_b620_1069b44d5646 = []
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_88591603_b9dc_329d_b620_1069b44d5646))

    def err(self, err_code:int):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_88591603_b9dc_329d_b620_1069b44d5646 = []
        _argv_88591603_b9dc_329d_b620_1069b44d5646.append(err_code)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_88591603_b9dc_329d_b620_1069b44d5646))

class player_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_into_scene:list[Callable[[player_into_scene_rsp, str, int], None]] = []
        self.entity.reg_client_request_callback("into_scene", self.into_scene)
        self.on_talk_npc:list[Callable[[player_talk_npc_rsp, int], None]] = []
        self.entity.reg_client_request_callback("talk_npc", self.talk_npc)
        self.on_get_task_info:list[Callable[[session, ], None]] = []
        self.entity.reg_client_notify_callback("get_task_info", self.get_task_info)
        self.on_completed_task:list[Callable[[session, int], None]] = []
        self.entity.reg_client_notify_callback("completed_task", self.completed_task)

    def into_scene(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _scene_name = inArray[0]
        _scene_line = inArray[1]
        rsp = player_into_scene_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_into_scene:
            fn(rsp, _scene_name, _scene_line)

    def talk_npc(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _talk_id = inArray[0]
        rsp = player_talk_npc_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_talk_npc:
            fn(rsp, _talk_id)

    def get_task_info(self, gate_name:str, bin:bytes):
        inArray = loads(bin)
        s = session(gate_name)
        for fn in self.on_get_task_info:
            fn(s, )

    def completed_task(self, gate_name:str, bin:bytes):
        inArray = loads(bin)
        _task_id = inArray[0]
        s = session(gate_name)
        for fn in self.on_completed_task:
            fn(s, _task_id)




