from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
class vec3(object):
    def __init__(self):
        self.x:float = 0.0
        self.y:float = 0.0
        self.z:float = 0.0


def vec3_to_protcol(_struct:vec3):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["x"] = _struct.x
    _protocol["y"] = _struct.y
    _protocol["z"] = _struct.z
    return _protocol

def protcol_to_vec3(_protocol:dict):
    _struct = vec3()
    for (key, val) in _protocol.items():
        if key == "x":
            _struct.x = val
        elif key == "y":
            _struct.y = val
        elif key == "z":
            _struct.z = val
    return _struct

class quat(object):
    def __init__(self):
        self.x:float = 0.0
        self.y:float = 0.0
        self.z:float = 0.0
        self.w:float = 0.0


def quat_to_protcol(_struct:quat):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["x"] = _struct.x
    _protocol["y"] = _struct.y
    _protocol["z"] = _struct.z
    _protocol["w"] = _struct.w
    return _protocol

def protcol_to_quat(_protocol:dict):
    _struct = quat()
    for (key, val) in _protocol.items():
        if key == "x":
            _struct.x = val
        elif key == "y":
            _struct.y = val
        elif key == "z":
            _struct.z = val
        elif key == "w":
            _struct.w = val
    return _struct

class body_snapshot(object):
    def __init__(self):
        self.id:str = ""
        self.position:vec3 = None
        self.rotation:quat = None


def body_snapshot_to_protcol(_struct:body_snapshot):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["id"] = _struct.id
    _protocol["position"] = vec3_to_protcol(_struct.position)
    _protocol["rotation"] = quat_to_protcol(_struct.rotation)
    return _protocol

def protcol_to_body_snapshot(_protocol:dict):
    _struct = body_snapshot()
    for (key, val) in _protocol.items():
        if key == "id":
            _struct.id = val
        elif key == "position":
            _struct.position = protcol_to_vec3(val)
        elif key == "rotation":
            _struct.rotation = protcol_to_quat(val)
    return _struct

class ray_hit(object):
    def __init__(self):
        self.body_id:str = ""
        self.point:vec3 = None
        self.normal:vec3 = None


def ray_hit_to_protcol(_struct:ray_hit):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["body_id"] = _struct.body_id
    _protocol["point"] = vec3_to_protcol(_struct.point)
    _protocol["normal"] = vec3_to_protcol(_struct.normal)
    return _protocol

def protcol_to_ray_hit(_protocol:dict):
    _struct = ray_hit()
    for (key, val) in _protocol.items():
        if key == "body_id":
            _struct.body_id = val
        elif key == "point":
            _struct.point = protcol_to_vec3(val)
        elif key == "normal":
            _struct.normal = protcol_to_vec3(val)
    return _struct

class contact_info(object):
    def __init__(self):
        self.body1_id:str = ""
        self.body2_id:str = ""
        self.point:vec3 = None
        self.normal:vec3 = None


def contact_info_to_protcol(_struct:contact_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["body1_id"] = _struct.body1_id
    _protocol["body2_id"] = _struct.body2_id
    _protocol["point"] = vec3_to_protcol(_struct.point)
    _protocol["normal"] = vec3_to_protcol(_struct.normal)
    return _protocol

def protcol_to_contact_info(_protocol:dict):
    _struct = contact_info()
    for (key, val) in _protocol.items():
        if key == "body1_id":
            _struct.body1_id = val
        elif key == "body2_id":
            _struct.body2_id = val
        elif key == "point":
            _struct.point = protcol_to_vec3(val)
        elif key == "normal":
            _struct.normal = protcol_to_vec3(val)
    return _struct

#this module code is codegen by geese codegen for python


