from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class notice_caller(object):
    def __init__(self):
        pass

    def notice(self, msg:str):
        _argv_20b10824_d060_350b_9c34_134701265d2a = []
        _argv_20b10824_d060_350b_9c34_134701265d2a.append(msg)
        global_entity().call_client("notice", dumps(_argv_20b10824_d060_350b_9c34_134701265d2a))



