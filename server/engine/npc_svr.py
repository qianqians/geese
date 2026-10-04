from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class npc_accept_task_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, tasks:list[task_info]):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1 = []
        _list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870 = []
        for v_9b78b6ab_a6db_5ec0_a23e_7f18c1d576b5 in tasks:
            _list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870.append(task_info_to_protcol(v_9b78b6ab_a6db_5ec0_a23e_7f18c1d576b5))
        _argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1.append(_list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1 = []
        _argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_300a447a_63f4_3f9b_8d00_4f2be030e6f1))

class npc_complete_task_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, tasks:list[task_info]):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8 = []
        _list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870 = []
        for v_9b78b6ab_a6db_5ec0_a23e_7f18c1d576b5 in tasks:
            _list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870.append(task_info_to_protcol(v_9b78b6ab_a6db_5ec0_a23e_7f18c1d576b5))
        _argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8.append(_list_ff486e7f_b5aa_3a8b_9d23_28dd187f4870)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8 = []
        _argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_24f1bbd4_24d5_33cf_82be_64a44d300cd8))

class npc_purchase_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, items:list[item]):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b = []
        _list_345fb533_0c2c_3ec8_899f_fe985da7fd3a = []
        for v_bf5596fb_837a_5272_bf0d_cd29c7c99192 in items:
            _list_345fb533_0c2c_3ec8_899f_fe985da7fd3a.append(item_to_protcol(v_bf5596fb_837a_5272_bf0d_cd29c7c99192))
        _argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b.append(_list_345fb533_0c2c_3ec8_899f_fe985da7fd3a)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b = []
        _argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e0c7fe5d_7700_3643_ac3f_27cfc058984b))

class npc_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_accept_task:list[Callable[[npc_accept_task_rsp, int], None]] = []
        self.entity.reg_client_request_callback("accept_task", self.accept_task)
        self.on_complete_task:list[Callable[[npc_complete_task_rsp, int], None]] = []
        self.entity.reg_client_request_callback("complete_task", self.complete_task)
        self.on_purchase:list[Callable[[npc_purchase_rsp, str], None]] = []
        self.entity.reg_client_request_callback("purchase", self.purchase)

    def accept_task(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _task_id = inArray[0]
        rsp = npc_accept_task_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_accept_task:
            fn(rsp, _task_id)

    def complete_task(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _task_id = inArray[0]
        rsp = npc_complete_task_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_complete_task:
            fn(rsp, _task_id)

    def purchase(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _item_id = inArray[0]
        rsp = npc_purchase_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_purchase:
            fn(rsp, _item_id)




