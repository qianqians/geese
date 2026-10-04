from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class scene_ntf_client_caller(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

    def move(self, pos:position_info):
        _argv_33efb72e_9227_32af_a058_169be114a277 = []
        _argv_33efb72e_9227_32af_a058_169be114a277.append(position_info_to_protcol(pos))
        self.entity.call_client_mutilcast("move", dumps(_argv_33efb72e_9227_32af_a058_169be114a277))

    def entity_refresh(self, info:bytes):
        _argv_4eb116e8_84f3_372b_9f0d_333edb12fc26 = []
        _argv_4eb116e8_84f3_372b_9f0d_333edb12fc26.append(info)
        self.entity.call_client_mutilcast("entity_refresh", dumps(_argv_4eb116e8_84f3_372b_9f0d_333edb12fc26))



