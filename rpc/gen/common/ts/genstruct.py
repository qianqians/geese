#coding:utf-8
# 2023-3-31
# build by qianqians
# genstruct

from tools.ts.tools import *
from tools.ts.gen_tools import *

def genmainstruct(struct_name, elems, dependent_struct, dependent_enum, enum):
    code = "export class " + struct_name + " {\n"
    names = []
    for key, value, parameter in elems:
        if value in names:
            raise Exception("repeat struct elem:%s in struct:%s" % (key, struct_name))
        names.append(value)
        if parameter == None:
            code += "     public " + value + ":" + convert_type(key, dependent_struct, dependent_enum) + " = " + default_parameter(key, dependent_struct, dependent_enum, enum) + "\n"
        else:
            code += "     public " + value + ":" + convert_type(key, dependent_struct, dependent_enum) + " = " + convert_parameter(key, parameter, dependent_enum, enum) + "\n"
    code += "}\n\n"
    return code

def genstructprotocol(struct_name, elems, dependent_struct, dependent_enum):
    code = "export function " + struct_name + "_to_protcol(_struct:" + struct_name + ") {\n"
    code += "    let _protocol:any = {}\n"
    for key, value, parameter in elems:
        type_ = check_type(key, dependent_struct, dependent_enum)
        if check_type_original(type_):
            code += "    _protocol[\"" + value + "\"] = _struct." + value + "\n"
        elif type_ == TypeType.Custom:
            code += "    _protocol[\"" + value + "\"] = " + key + "_to_protcol(_struct." + value + ")\n"
        elif type_ == TypeType.List:
            code += "    if (_struct." + value + ") {\n"
            code += "        _array_" + value + " = []\n"
            code += "        for (let v_ of _struct." + value + ") {\n"
            array_type = key[5:-1]
            code += gen_struct_container_protocol(3, "_array_" + value, "list", array_type, "", "v_", dependent_struct, dependent_enum)
            code += "        }\n"
            code += "        _protocol[\"" + value + "\"] = _array_" + value + "\n"
            code += "    }\n"
    code += "    return _protocol;\n"
    code += "}\n\n"
    return code

def genprotocolstruct(struct_name, elems, dependent_struct, dependent_enum):
    code = "export function protcol_to_" + struct_name + "(_protocol:any) {\n"
    code += "    let _struct = new " + struct_name + "()\n"
    code += "    for (let key in _protocol) {\n"
    code += "        let val = _protocol[key];\n"
    count = 0
    for key, value, parameter in elems:
        type_ = check_type(key, dependent_struct, dependent_enum)
        _type = convert_type(key, dependent_struct, dependent_enum)
        if count == 0:
            code += "        if (key == \"" + value + "\") {\n"
        else:
            code += "        else if (key == \"" + value + "\") {\n"
        if check_type_original(type_):
            code += "            _struct." + value + " = val;\n"
        elif type_ == TypeType.Custom:
            code += "            _struct." + value + " = protcol_to_" + key + "(val);\n"
        elif type_ == TypeType.List:
            code += "            _struct." + value + " = []\n"
            code += "            for (let v_ of val) {\n"
            array_type = key[5:-1]
            code += gen_struct_protocol_container(4, "_struct." + value, "list", array_type, "", "v_", dependent_struct, dependent_enum)
            code += "            }\n"
        count = count + 1
        code += "        }\n"
    code += "    }\n"
    code += "    return _struct;\n\n"
    code += "}\n\n"
    return code

def genstruct(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    struct = pretreatment.struct
    
    code = "// this struct code is codegen by geese codegen for ts\n"
    for struct_name, elems in struct.items():
        code += genmainstruct(struct_name, elems, dependent_struct, dependent_enum, pretreatment.all_enum)
        code += genstructprotocol(struct_name, elems, dependent_struct, dependent_enum)
        code += genprotocolstruct(struct_name, elems, dependent_struct, dependent_enum)

    return code
