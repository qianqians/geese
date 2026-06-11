from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .physics_common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class physics_cast_ray_rsp(session):
    def __init__(self, gate_name:str, conn_id:str, msg_cb_id:int, entity:player|entity):
        session.__init__(self, gate_name)
        self.entity = entity
        self.conn_id = conn_id
        self.is_rsp = False
        self.msg_cb_id = msg_cb_id

    def rsp(self, hit:ray_hit, has_hit:bool):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f = []
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(ray_hit_to_protcol(hit))
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(has_hit)
        self.entity.call_client_response(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f))

    def err(self, err:str):
        if self.is_rsp:
            return
        self.is_rsp = True

        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f = [self.uuid_5b9b2c5e_7009_3cca_bb95_22fd524deeda]
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(err)
        self.entity.call_client_response_error(self.source, self.conn_id, self.msg_cb_id, dumps(_argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f))

class physics_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_cast_ray:list[Callable[[physics_cast_ray_rsp, float, float, float, float, float, float, float], None]] = []
        self.entity.reg_client_request_callback("cast_ray", self.cast_ray)

    def cast_ray(self, gate_name:str, conn_id:str, msg_cb_id:int, bin:bytes):
        inArray = loads(bin)
        _ox = inArray[0]
        _oy = inArray[1]
        _oz = inArray[2]
        _dx = inArray[3]
        _dy = inArray[4]
        _dz = inArray[5]
        _max_toi = inArray[6]
        rsp = physics_cast_ray_rsp(gate_name, conn_id, msg_cb_id, self.entity)
        for fn in self.on_cast_ray:
            fn(rsp, _ox, _oy, _oz, _dx, _dy, _dz, _max_toi)




