#coding:utf-8
# 2023-3-31
# build by qianqians
# gen_tools

import uuid
from .tools import *

def gen_list_type_code_type_to_protcol(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _key:str,
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "

    _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, _name)).split('-'))
    code = space + "_list_" + _argv_uuid + " = []\n"

    _child_type = _type[5:-1]
    _child_type_ = check_type(_child_type, dependent_struct, dependent_enum)

    _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, _name)).split('-'))
    code += space + "for v_" + _v_uuid + " in " + _name + ":\n"
    code += gen_type_code_type_to_protcol(
        depth + 1, 
        "_list_" + _argv_uuid,
        "list", 
        _child_type, 
        _child_type_,
        "", 
        "v_" + _v_uuid, 
        func_name, 
        dependent_struct, 
        dependent_enum)

    if c_type == "list":
        code += space + container + ".append(_list_" + _argv_uuid + ")\n"

    return code

def gen_dict_type_code_type_to_protcol(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _key:str,
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "

    _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, _name)).split('-'))
    code = space + "_dict_" + _argv_uuid + " = {}\n"

    _child_type = _type[4:-1]
    _child_type_ = check_type(_child_type, dependent_struct, dependent_enum)

    _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, _name)).split('-'))
    code = space + "for k_" + _v_uuid + ", v_" + _v_uuid + " in " + _name + ".items():\n"
    code += gen_type_code_type_to_protcol(
        depth + 1, 
        "_list_" + _argv_uuid, 
        "dict",
        _child_type, 
        _child_type_, 
        "k_" + _v_uuid,
        "v_" + _v_uuid, 
        func_name, 
        dependent_struct, 
        dependent_enum)

    if c_type == "list":
        code += space + container + ".append(_dict_" + _argv_uuid + ")\n"

    return code

def gen_type_code_type_to_protcol(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _type_enum:int, 
        _key:str, 
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    
    if _type_enum == TypeType.List:
        return gen_list_type_code_type_to_protcol(
            depth, 
            container, 
            c_type, 
            _type, 
            _key,
            _name, 
            func_name, 
            dependent_struct, 
            dependent_enum)
    
    space = ""
    for i in range(depth):
        space += "    "
    if c_type == "list":
        if check_type_original(_type_enum):    
            return space + container + ".append(" + _name + ")\n"
        elif _type_enum == TypeType.Custom:
            return space + container + ".append(" + _type + "_to_protcol(" + _name + "))\n"
        elif _type_enum == TypeType.Custom:
            return space + container + "[\"" + _key + "\"] = " + _type + "_to_protcol(" + _name + "))\n"
    
    raise Exception("not support type:%s in func:%s" % (_type, func_name))

def gen_list_type_code_protcol_to_type(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _key:str,
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "

    _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, _name)).split('-'))
    code = space + "_list_" + _argv_uuid + " = []\n"

    _child_type = _type[5:-1]
    _child_type_ = check_type(_child_type, dependent_struct, dependent_enum)

    _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, _name)).split('-'))
    code = space + "for v_" + _v_uuid + " in " + _name + ":\n"
    code += gen_type_code_protcol_to_type(
        depth + 1, 
        "_list_" + _argv_uuid,
        "list", 
        _child_type, 
        _child_type_,
        "", 
        "v_" + _v_uuid, 
        func_name, 
        dependent_struct, 
        dependent_enum)

    if c_type == "list":
        code += space + container + ".append(_list_" + _argv_uuid + ")\n"

    return code

def gen_dict_type_code_protcol_to_type(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _key:str,
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "

    _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, _name)).split('-'))
    code = space + "_dict_" + _argv_uuid + " = {}\n"

    _child_type = _type[4:-1]
    _child_type_ = check_type(_child_type, dependent_struct, dependent_enum)

    _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, _name)).split('-'))
    code = space + "for k_" + _v_uuid + ", v_" + _v_uuid + " in " + _name + ".items():\n"
    code += gen_type_code_protcol_to_type(
        depth + 1, 
        "_list_" + _argv_uuid, 
        "dict",
        _child_type, 
        _child_type_, 
        "k_" + _v_uuid,
        "v_" + _v_uuid, 
        func_name, 
        dependent_struct, 
        dependent_enum)

    if c_type == "list":
        code += space + container + ".append(_dict_" + _argv_uuid + ")\n"

    return code

def gen_type_code_protcol_to_type(
        depth:int, 
        container:str, 
        c_type:str, 
        _type:str, 
        _type_enum:int, 
        _key:str, 
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    
    if _type_enum == TypeType.List:
        return gen_list_type_code_protcol_to_type(
            depth, 
            container, 
            c_type, 
            _type, 
            _key,
            _name, 
            func_name, 
            dependent_struct, 
            dependent_enum)
    
    space = ""
    for i in range(depth):
        space += "    "
    if c_type == "list":
        if check_type_original(_type_enum):
            return space + container + ".append(" + _name + ")\n"
        elif _type_enum == TypeType.Custom:
            return space + container + ".append(" + _type + "_to_protcol(" + _name + "))\n"
    
    raise Exception("not support type:%s in func:%s" % (_type, func_name))

def gen_type_code_module(
        depth:int, 
        _count:int, 
        _type:str, 
        _type_enum:int, 
        _name:str, 
        func_name:str, 
        dependent_struct, 
        dependent_enum):
    
    space = ""
    for i in range(depth):
        space += "    "

    if _type_enum == TypeType.List:
        _child_type = _type[5:-1]
        _child_type_ = check_type(_child_type, dependent_struct, dependent_enum)

        code = space + "_" + _name + " = []\n"
        _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, _name)).split('-'))
        code += space + "for v_" + _v_uuid + " in inArray[" + str(_count) + "]:\n"
        code += gen_type_code_protcol_to_type(
            depth + 1, 
            "_" + _name, 
            "list", 
            _child_type, 
            _child_type_,
            "", 
            "v_" + _v_uuid, 
            func_name, 
            dependent_struct, 
            dependent_enum)
        return code
    
    if check_type_original(_type_enum):
        return space + "_"  + _name + " = inArray[" + str(_count) + "]\n"
    elif _type_enum == TypeType.Custom:
        return space + "_"  + _name + " = protcol_to_" + _type + "(inArray[" + str(_count) + "])\n"
    

def gen_struct_container_protocol(depth:int, container:str, c_type:str, array_type:str, _key:str, value_name:str, dependent_struct, dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "
        
    _array_type_ = check_type(array_type, dependent_struct, dependent_enum)
    if _array_type_ == TypeType.List:
        _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, value_name)).split('-'))
        code = space + "_array_" + _v_uuid + " = []\n"
        code += space + "for v_" + _v_uuid + " in " + value_name + ":\n"
        _child_type = array_type[5:-1]
        code += gen_struct_container_protocol(depth + 1, "_array_" + _v_uuid, "list", _child_type, "", "v_" + _v_uuid, dependent_struct, dependent_enum)
        if c_type == "list":
            code += space + container + ".append(_array_" + _v_uuid + ")\n"
        return code

    if c_type == "list":
        if check_type_original(_array_type_):
            return space + container + ".append(" + value_name + ")\n"
        elif _array_type_ == TypeType.Custom:
            return space + container + ".append(" + array_type + "_to_protcol(" + value_name + "))\n"

def gen_struct_protocol_container(depth:int, container:str, c_type:str, array_type:str, _key:str, value_name:str, dependent_struct, dependent_enum):
    space = ""
    for i in range(depth):
        space += "    "
        
    _array_type_ = check_type(array_type, dependent_struct, dependent_enum)
    if _array_type_ == TypeType.List:
        _v_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_X500, value_name)).split('-'))
        code = space + "_array_" + _v_uuid + " = []\n"
        code += space + "for v_" + _v_uuid + " in " + value_name + ":\n"
        _child_type = array_type[5:-1]
        code += gen_struct_protocol_container(depth + 1, "_array_" + _v_uuid, "list", _child_type, "", "v_" + _v_uuid, dependent_struct, dependent_enum)
        if c_type == "list":
            code += space + container + ".append(_array_" + _v_uuid + ")\n"
        return code
    
    if c_type == "list":
        if check_type_original(_array_type_):
            return space + container + ".append(" + value_name + ")\n"
        elif _array_type_ == TypeType.Custom:
            return space + container + ".append(" + array_type + "_to_protcol(" + value_name + "))\n"
