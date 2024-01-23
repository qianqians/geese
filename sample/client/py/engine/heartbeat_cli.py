from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class heartbeat_call_heartbeat_rsp(session):
    def __init__(self, hub_name:str, msg_cb_id:int, entity:player):
        session.__init__(self, hub_name)
        self.entity = entity
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, time_info:client_time_info):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f = []
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f.append(client_time_info_to_protcol(time_info))
        self.entity.call_hub_response(self.msg_cb_id, dumps(_argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f))

    def err(self, err:error_code):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f = [self.uuid_bdac5a44_ce76_324d_b00d_2889c5d260ee]
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f.append(err)
        self.entity.call_hub_response_error(self.msg_cb_id, dumps(_argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f))

class heartbeat_module(object):
    def __init__(self, entity:player):
        self.entity = entity

        self.on_call_heartbeat:list[Callable[[heartbeat_call_heartbeat_rsp, str]]] = []
        self.entity.reg_hub_request_callback("call_heartbeat", self.call_heartbeat)

    def call_heartbeat(self, hub_name:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _entity_id = inArray[1]
        rsp = heartbeat_call_heartbeat_rsp(hub_name, msg_cb_id, self.entity)
        for fn in self.on_call_heartbeat:
            fn(rsp, _entity_id)



