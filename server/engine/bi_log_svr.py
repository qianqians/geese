from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class bi_log_caller(object):
    def __init__(self, entity:subentity):
        self.entity = entity

    def bi(self, log:str):
        _argv_c229202a_195a_3008_ae59_52daf240ef5e = []
        _argv_c229202a_195a_3008_ae59_52daf240ef5e.append(log)
        self.entity.call_hub_notify("bi", dumps(_argv_c229202a_195a_3008_ae59_52daf240ef5e))

#this module code is codegen by geese codegen for python
class bi_log_module(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

        self.on_bi:list[Callable[[session, str], None]] = []
        self.entity.reg_hub_notify_callback("bi", self.bi)

    def bi(self, source:str, bin:bytes):
        inArray = loads(bin)
        _log = inArray[0]
        s = session(source)
        for fn in self.on_bi:
            fn(s, _log)



