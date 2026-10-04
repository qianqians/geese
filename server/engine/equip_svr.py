from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class equip_wear_equip_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, equips:list[equip_info]):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_29899057_23ab_3371_9bff_b3c5a5aec1a2 = []
        _list_d335ace6_2d27_3974_a575_cf77cedfa576 = []
        for v_10d8619a_ef87_53c6_b028_8501d96c0af7 in equips:
            _list_d335ace6_2d27_3974_a575_cf77cedfa576.append(equip_info_to_protcol(v_10d8619a_ef87_53c6_b028_8501d96c0af7))
        _argv_29899057_23ab_3371_9bff_b3c5a5aec1a2.append(_list_d335ace6_2d27_3974_a575_cf77cedfa576)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_29899057_23ab_3371_9bff_b3c5a5aec1a2))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_29899057_23ab_3371_9bff_b3c5a5aec1a2 = []
        _argv_29899057_23ab_3371_9bff_b3c5a5aec1a2.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_29899057_23ab_3371_9bff_b3c5a5aec1a2))

class equip_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_wear_equip:list[Callable[[equip_wear_equip_rsp, str], None]] = []
        self.entity.reg_client_request_callback("wear_equip", self.wear_equip)

    def wear_equip(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _equip_id = inArray[0]
        rsp = equip_wear_equip_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_wear_equip:
            fn(rsp, _equip_id)




