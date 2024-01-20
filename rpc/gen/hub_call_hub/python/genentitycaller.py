#coding:utf-8
# 2020-1-21
# build by qianqians
# gencaller

import uuid
from tools.python.tools import *
from tools.python.gen_tools import *

def gen_entity_caller(module_name, funcs, dependent_struct, dependent_enum, enum):
    code = "class " + module_name + "_caller(object):\n"
    code += "    def __init__(self, entity:subentity):\n"
    code += "        self.entity = entity\n\n"

    cb_code = ""

    for i in funcs:
        func_name = i[0]

        if i[1] == "ntf":
            code += "    def " + func_name + "(self, "
            count = 0
            for _type, _name, _parameter in i[2]:
                if _parameter == None:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[2]):
                    code += ", "
            code += "):\n"
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            code += "        _argv_" + _argv_uuid + " = []\n"
            for _type, _name, _parameter in i[2]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                code += gen_type_code_type_to_protcol(
                    2, 
                    "_argv_" + _argv_uuid, 
                    "list", 
                    _type, 
                    type_, 
                    "", 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
            code += "        self.entity.call_hub_notify(\"" + func_name + "\", dumps(_argv_" + _argv_uuid + "))\n\n"
        elif i[1] == "req" and i[3] == "rsp" and i[5] == "err":
            rsp_fn = "Callable[["
            count = 0
            for _type, _name, _parameter in i[4]:
                rsp_fn += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[4]):
                    rsp_fn += ", "
            rsp_fn += "]]"
            
            err_fn = "Callable[["
            count = 0
            for _type, _name, _parameter in i[6]:
                err_fn += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[6]):
                    err_fn += ", "
            err_fn += "]]"

            cb_code += "class " + module_name + "_" + func_name + "_cb(object):\n"
            cb_code += "    def __init__(self, _cb_uuid:int, _entity:subentity):\n"
            cb_code += "        self.entity = _entity\n"
            cb_code += "        self.cb:" + rsp_fn + " = None\n"
            cb_code += "        self.err:" + err_fn + " = None\n"
            cb_code += "        self.rsp = callback(lambda: self.entity.del_callback(_cb_uuid))\n"
            cb_code += "        self.entity.reg_hub_callback(_cb_uuid, self.rsp)\n\n"

            cb_code += "    def on_rsp(self, bin:bytes):\n"
            cb_code += "        inArray = loads(bin)\n"
            count = 0
            for _type, _name, _parameter in i[4]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                cb_code += gen_type_code_module(
                    2, 
                    count, 
                    _type, 
                    type_, 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
                count += 1
            cb_code += "        self.cb("
            count = 0
            for _type, _name, _parameter in i[4]:
                cb_code += "_" + _name
                count = count + 1
                if count < len(i[4]):
                    cb_code += ", "
            cb_code += ")\n\n"

            cb_code += "    def on_err(self, bin:bytes):\n"
            cb_code += "        inArray = loads(bin)\n"
            count = 0
            for _type, _name, _parameter in i[6]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                cb_code += gen_type_code_module(
                    2, 
                    count, 
                    _type, 
                    type_, 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
                count += 1
            cb_code += "\n"
            cb_code += "        self.err("
            count = 0
            for _type, _name, _parameter in i[6]:
                cb_code += "_" + _name
                count = count + 1
                if count < len(i[4]):
                    cb_code += ", "
            cb_code += ")\n\n"

            cb_code += "    def callBack(self, _cb:" + rsp_fn + ", _err:" + err_fn + "):\n"
            cb_code += "        self.cb = _cb\n"
            cb_code += "        self.err = _err\n"
            cb_code += "        self.rsp.callback(self.on_rsp, self.on_err)\n"
            cb_code += "        return self.rsp\n\n"

            _cb_uuid_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_DNS, func_name)).split('-'))
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))

            code += "    def " + func_name + "(self, "
            count = 0
            for _type, _name, _parameter in i[2]:
                if _parameter == None:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[2]):
                    code += ", "
            code += "):\n"
            code += "        _argv_" + _argv_uuid + " = []\n"
            for _type, _name, _parameter in i[2]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                code += gen_type_code_type_to_protcol(
                    2, 
                    "_argv_" + _argv_uuid, 
                    "list", 
                    _type, 
                    type_, 
                    "", 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
            code += "        _cb_uuid = self.entity.call_hub_request(\"" + func_name + "\", dumps(_argv_" + _argv_uuid + "))\n\n"
            code += "        return " + module_name + "_" + func_name + "_cb(_cb_uuid, self)\n\n"

        else:
            raise Exception("func:" + func_name + " wrong rpc type:" + str(i[1]) + ", must req or ntf")

    return cb_code + code