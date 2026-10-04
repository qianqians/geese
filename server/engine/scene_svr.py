from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class scene_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_move:list[Callable[[session, direction], None]] = []
        self.entity.reg_client_notify_callback("move", self.move)

    def move(self, gate_name:str, bin:bytes):
        inArray = loads(bin)
        _dir = inArray[0]
        s = session(gate_name)
        for fn in self.on_move:
            fn(s, _dir)




