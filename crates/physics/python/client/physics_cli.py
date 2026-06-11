from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .physics_common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class physics_cast_ray_cb(object):
    def __init__(self, _cb_uuid:int, _entity:subentity|player):
        self.entity = _entity
        self.cb:Callable[[ray_hit, bool], None] = None
        self.err:Callable[[str], None] = None
        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))
        self.entity.reg_hub_callback(_cb_uuid, self.rsp)

    def on_rsp(self, bin:bytes):
        inArray = loads(bin)
        _hit = protcol_to_ray_hit(inArray[0])
        _has_hit = inArray[1]
        self.cb(_hit, _has_hit)

    def on_err(self, bin:bytes):
        inArray = loads(bin)
        _err = inArray[0]
        self.err(_err, )

    def callBack(self, _cb:Callable[[ray_hit, bool], None], _err:Callable[[str], None]):
        self.cb = _cb
        self.err = _err
        self.rsp.callback(self.on_rsp, self.on_err)
        return self.rsp

class physics_caller(object):
    def __init__(self, entity:subentity|player):
        self.entity = entity

    def cast_ray(self, ox:float, oy:float, oz:float, dx:float, dy:float, dz:float, max_toi:float):
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f = []
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(ox)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(oy)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(oz)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(dx)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(dy)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(dz)
        _argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f.append(max_toi)
        _cb_uuid = self.entity.call_hub_request("cast_ray", dumps(_argv_06ebe4c8_7335_313c_95d3_2e2cd950ba2f))

        return physics_cast_ray_cb(_cb_uuid, self.entity)



