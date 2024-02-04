from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class notice_module(object):
    def __init__(self):
        pass

        self.on_notice:list[Callable[[session, str], None]] = []
        app().register_global_method("notice", self.notice)

    def notice(self, hub_name:str, bin:bytes):
        inArray = loads(bin)
        _msg = inArray[0]
        s = session(hub_name)
        for fn in self.on_notice:
            fn(s, _msg)




