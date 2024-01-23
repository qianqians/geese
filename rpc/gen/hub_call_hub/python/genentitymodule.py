#coding:utf-8
# 2020-1-21
# build by qianqians
# genmodule

import uuid
from tools.python.tools import *
from tools.python.gen_tools import *

def gen_entity_module(module_name, funcs, dependent_struct, dependent_enum, enum):
    code_constructor = "class " + module_name + "_module(object):\n"
    code_constructor += "    def __init__(self, entity:player|entity):\n"
    code_constructor += "        self.entity = entity\n\n"
        
    rsp_code = ""
    code_func = ""
    for i in funcs:
        func_name = i[0]
        if i[1] == "ntf":
            func_type = "Callable[[session, "
            count = 0
            for _type, _name, _parameter in i[2]:
                func_type += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[2]):
                    func_type += ", "
            func_type += "], None]"
            
            code_constructor += "        self.on_" + func_name + ":list[" + func_type + "] = []\n"
            code_constructor += "        self.entity.reg_hub_notify_callback(\"" + func_name + "\", self." + func_name + ")\n"

            code_func += "    def " + func_name + "(self, source:str, bin:bytes):\n"
            code_func += "        inArray = loads(bin)\n"
            count = 0 
            for _type, _name, _parameter in i[2]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                code_func += gen_type_code_module(
                    2, 
                    count, 
                    _type, 
                    type_, 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
                count += 1
            code_func += "        s = session(source)\n"
            code_func += "        for fn in self.on_" + func_name + ":\n"
            code_func += "            fn(s, "
            count = 0
            for _type, _name, _parameter in i[2]:
                code_func += "_" + _name
                count = count + 1
                if count < len(i[2]):
                    code_func += ", "
            code_func += ")\n\n"
        elif i[1] == "req" and i[3] == "rsp" and i[5] == "err":
            func_type = "Callable[[" + module_name + "_" + func_name + "_rsp, "
            count = 0
            for _type, _name, _parameter in i[2]:
                func_type += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[2]):
                    func_type += ", "
            func_type += "], None]"
            
            code_constructor += "        self.on_" + func_name + ":list[" + func_type + "] = []\n"
            code_constructor += "        self.entity.reg_hub_request_callback(\"" + func_name + "\", self." + func_name + ")\n"
            
            code_func += "    def " + func_name + "(self, source:str, msg_cb_id:int, bin:bytes):\n"
            code_func += "        inArray = loads(bin)\n"
            count = 1 
            for _type, _name, _parameter in i[2]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                type_ = check_type(_type, dependent_struct, dependent_enum)
                code_func += gen_type_code_module(
                    2, 
                    count, 
                    _type, 
                    type_, 
                    _name, 
                    func_name, 
                    dependent_struct, 
                    dependent_enum)
                count += 1
            code_func += "        rsp = " + module_name + "_" + func_name + "_rsp(source, msg_cb_id)\n"
            code_func += "        for fn in self.on_" + func_name + ":\n"
            code_func += "            fn(rsp, "
            count = 0
            for _type, _name, _parameter in i[2]:
                code_func += "_" + _name
                count = count + 1
                if count < len(i[2]):
                    code_func += ", "
            code_func += ")\n"

            _hub_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            _rsp_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_X500, func_name)).split('-'))
            rsp_code += "class " + module_name + "_" + func_name + "_rsp(session):\n"
            rsp_code += "    def __init__(self, source:str, msg_cb_id:int, entity:player|entity):\n"
            rsp_code += "        session.__init__(self, source)\n"
            rsp_code += "        self.entity = entity\n"
            rsp_code += "        self.is_rsp = False\n"
            rsp_code += "        self.msg_cb_id = msg_cb_id\n\n"

            rsp_code += "    def rsp(self, "
            count = 0
            for _type, _name, _parameter in i[4]:
                if _parameter == None:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[4]):
                    rsp_code += ", "
            rsp_code += "):\n"
            rsp_code += "        if self.is_rsp:\n"
            rsp_code += "            return\n"
            rsp_code += "        self.is_rsp = True\n\n"
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            rsp_code += "        _argv_" + _argv_uuid + " = []\n"
            for _type, _name, _parameter in i[4]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                rsp_code += gen_type_code_type_to_protcol(
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
            rsp_code += "        self.entity.call_hub_response(self.source, self.msg_cb_id, dumps(_argv_" + _argv_uuid + "))\n\n"

            rsp_code += "    def err(self, "
            count = 0
            for _type, _name, _parameter in i[6]:
                if _parameter == None:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[6]):
                    rsp_code += ", "
            rsp_code += "):\n"
            rsp_code += "        if self.is_rsp:\n"
            rsp_code += "            return\n"
            rsp_code += "        self.is_rsp = True\n\n"
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            rsp_code += "        _argv_" + _argv_uuid + " = [self.uuid_" + _rsp_uuid + "]\n"
            for _type, _name, _parameter in i[6]:
                type_ = check_type(_type, dependent_struct, dependent_enum)
                rsp_code += gen_type_code_type_to_protcol(
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
            rsp_code += "        self.entity.call_hub_response_error(self.source, self.msg_cb_id, dumps(_argv_" + _argv_uuid + "))\n\n"

        else:
            raise Exception("func:%s wrong rpc type:%s must req or ntf" % (func_name, str(i[1])))

    code_constructor_end = "\n"
    code = "\n"
        
    return rsp_code + code_constructor + code_constructor_end + code_func + code
        