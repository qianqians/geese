#coding:utf-8
# 2023-9-17
# build by qianqians
# gencaller

import uuid
from tools.python.tools import *
from tools.python.gen_tools import *

def gen_global_caller(module_name, funcs, dependent_struct, dependent_enum, enum):
    code = "class " + module_name + "_caller(object):\n"
    code += "    def __init__(self):\n"
    code += "        pass\n\n"

    for i in funcs:
        func_name = i[0]

        if i[1] == "ntf":
            code += "    def " + func_name + "(self"
            count = 0
            for _type, _name, _parameter in i[2]:
                code += ", "
                if _parameter == None:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
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
            code += "        global_entity.instance().call_client(\"" + func_name + "\", dumps(_argv_" + _argv_uuid + "))\n\n"

        else:
            raise Exception("func:" + func_name + " wrong rpc type:" + str(i[1]) + ", must ntf")
        
    return code