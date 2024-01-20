from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class get_rank_get_self_rank_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, rank:role_rank_info):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e22ae90d_2428_3197_a8fb_549203f714e0 = []
        _argv_e22ae90d_2428_3197_a8fb_549203f714e0.append(role_rank_info_to_protcol(rank))
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e22ae90d_2428_3197_a8fb_549203f714e0))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e22ae90d_2428_3197_a8fb_549203f714e0 = [self.uuid_bff5b988_bcec_3884_b455_04b0df3c021f]
        _argv_e22ae90d_2428_3197_a8fb_549203f714e0.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e22ae90d_2428_3197_a8fb_549203f714e0))

class get_rank_get_rank_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, rank_list:list[role_rank_info]):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7 = []
        _list_97e71bcb_581e_338e_b072_0ef986ce8722 = []
        for v_e249ecbd_d64c_526b_901b_6d4ddee4b75a in rank_list:
            _list_97e71bcb_581e_338e_b072_0ef986ce8722.append(role_rank_info_to_protcol(v_e249ecbd_d64c_526b_901b_6d4ddee4b75a))
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.append(_list_97e71bcb_581e_338e_b072_0ef986ce8722)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e869f1c8_1f14_384f_aba6_2af2b54335e7))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7 = [self.uuid_9762adaa_b308_3609_9584_d6a45884469b]
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_e869f1c8_1f14_384f_aba6_2af2b54335e7))

class get_rank_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_get_self_rank:list[Callable[[get_rank_get_self_rank_rsp, str]]] = []
        self.entity.reg_client_request_callback("get_self_rank", self.get_self_rank)
        self.on_get_rank:list[Callable[[get_rank_get_rank_rsp, int, int]]] = []
        self.entity.reg_client_request_callback("get_rank", self.get_rank)

    def get_self_rank(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _entity_id = inArray[1]
        rsp = get_rank_get_self_rank_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_get_self_rank:
            fn(rsp, _entity_id)
    def get_rank(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _start = inArray[1]
        _end = inArray[2]
        rsp = get_rank_get_rank_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_get_rank:
            fn(rsp, _start, _end)



