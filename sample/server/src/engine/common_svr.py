from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *

# this enum code is codegen by geese codegen for python

class error_code(Enum):
    success = 0


#this struct code is codegen by geese codegen for python
class role_rank_info(object):
    def __init__(self):
        self.role_name:str = ""
        self.entity_id:str = ""
        self.rank:int = 0


def role_rank_info_to_protcol(_struct:role_rank_info):
    _protocol = {}
    _protocol["role_name"] = _struct.role_name
    _protocol["entity_id"] = _struct.entity_id
    _protocol["rank"] = _struct.rank
    return _protocol

def protcol_to_role_rank_info(_protocol:any):
    _struct = role_rank_info()
    for key, val in _protocol:
        if key == "role_name":
            _struct.role_name = val
        elif key == "entity_id":
            _struct.entity_id = val
        elif key == "rank":
            _struct.rank = val
    return _struct

class client_time_info(object):
    def __init__(self):
        self.entity_id:str = ""
        self.timetmp:int = 0


def client_time_info_to_protcol(_struct:client_time_info):
    _protocol = {}
    _protocol["entity_id"] = _struct.entity_id
    _protocol["timetmp"] = _struct.timetmp
    return _protocol

def protcol_to_client_time_info(_protocol:any):
    _struct = client_time_info()
    for key, val in _protocol:
        if key == "entity_id":
            _struct.entity_id = val
        elif key == "timetmp":
            _struct.timetmp = val
    return _struct

#this caller code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python

