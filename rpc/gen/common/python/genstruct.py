#coding:utf-8
# 2023-3-31
# build by qianqians
# genstruct

from tools.python.tools import *
from tools.python.gen_tools import *

def genmainstruct(struct_name, elems, dependent_struct, dependent_enum, enum):
    code = "class " + struct_name + "(object):\n"
    code += "    def __init__(self):\n"
    names = []
    for key, value, parameter in elems:
        if value in names:
            raise Exception("repeat struct elem:%s in struct:%s" % (key, struct_name))
        names.append(value)
        if parameter == None:
            code += "        self." + value + ":" + convert_type(key, dependent_struct, dependent_enum) + " = " + get_type_default(key, dependent_struct, dependent_enum) + "\n"
        else:
            code += "        self." + value + ":" + convert_type(key, dependent_struct, dependent_enum) + " = " + convert_parameter(key, parameter, dependent_enum, enum) + "\n"
    code += "\n\n"
    return code

def genstructprotocol(struct_name, elems, dependent_struct, dependent_enum):
    code = "def " + struct_name + "_to_protcol(_struct:" + struct_name + "):\n"
    code += "    if _struct is None:\n"
    code += "        return None\n"
    code += "    _protocol = {}\n"
    for key, value, parameter in elems:
        type_ = check_type(key, dependent_struct, dependent_enum)
        if check_type_original(type_):
            code += "    _protocol[\"" + value + "\"] = _struct." + value + "\n"
        elif type_ == TypeType.Custom:
            code += "    _protocol[\"" + value + "\"] = " + key + "_to_protcol(_struct." + value + ")\n"
        elif type_ == TypeType.Array:
            code += "    if _struct." + value + ":\n"
            code += "        _array_" + value + " = []\n"
            code += "        for v_ in _struct." + value + ":\n"
            array_type = key[5:-1]
            code += gen_struct_container_protocol(3, "_array_" + value, "list", array_type, "", "v_", dependent_struct, dependent_enum)
            code += "        _protocol[\"" + value + "\"] = _array_" + value + "\n"
        elif type_ == TypeType.Dict:
            code += "    if _struct." + value + ":\n"
            code += "        _dict_" + value + " = {}\n"
            code += "        for k_, v_ in _struct." + value + ".items():\n"
            dict_type = key[4:-1]
            code += gen_struct_container_protocol(3, "_dict_" + value, "dict", dict_type, "k_", "v_", dependent_struct, dependent_enum)
            code += "        _protocol[\"" + value + "\"] = _dict_" + value + "\n"
    code += "    return _protocol\n\n"
    return code

def genprotocolstruct(struct_name, elems, dependent_struct, dependent_enum):
    code = "def protcol_to_" + struct_name + "(_protocol:dict):\n"
    code += "    _struct = " + struct_name + "()\n"
    code += "    for (key, val) in _protocol.items():\n"
    count = 0
    for key, value, parameter in elems:
        type_ = check_type(key, dependent_struct, dependent_enum)
        _type = convert_type(key, dependent_struct, dependent_enum)
        if count == 0:
            code += "        if key == \"" + value + "\":\n"
        else:
            code += "        elif key == \"" + value + "\":\n"
        if check_type_original(type_):
            code += "            _struct." + value + " = val\n"
        elif type_ == TypeType.Custom:
            code += "            _struct." + value + " = protcol_to_" + key + "(val)\n"
        elif type_ == TypeType.Array:
            code += "            _struct." + value + " = []\n"
            code += "            for v_ in val:\n"
            array_type = key[5:-1]
            code += gen_struct_protocol_container(4, "_struct." + value, "list", array_type, "", "v_", dependent_struct, dependent_enum)
        elif type_ == TypeType.Dict:
            code += "            _struct." + value + " = {}\n"
            code += "            for k_, v_ in val:\n"
            dict_type = key[4:-1]
            code += gen_struct_protocol_container(4, "_struct." + value, "dict", dict_type, "k_", "v_", dependent_struct, dependent_enum)
        count = count + 1
    code += "    return _struct\n\n"
    return code

def genstruct(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    struct = pretreatment.struct
    
    code = "#this struct code is codegen by geese codegen for python\n"
    for struct_name, elems in struct.items():
        code += genmainstruct(struct_name, elems, dependent_struct, dependent_enum, pretreatment.all_enum)
        code += genstructprotocol(struct_name, elems, dependent_struct, dependent_enum)
        code += genprotocolstruct(struct_name, elems, dependent_struct, dependent_enum)

    return code
