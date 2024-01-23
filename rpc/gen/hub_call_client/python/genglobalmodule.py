#coding:utf-8
# 2023-9-17
# build by qianqians
# genmodule

import uuid
from tools.python.tools import *
from tools.python.gen_tools import *

def gen_global_module(module_name, funcs, dependent_struct, dependent_enum, enum):
    code_constructor = "class " + module_name + "_module(object):\n"
    code_constructor += "    def __init__(self):\n"
    code_constructor += "        pass\n\n"
        
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
            code_constructor += "        app.instance().register_global_method(\"" + func_name + "\", self." + func_name + ")\n"

            code_func += "    def " + func_name + "(self, hub_name:str, bin:bytes):\n"
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
            code_func += "        s = session(hub_name)\n"
            code_func += "        for fn in self.on_" + func_name + ":\n"
            code_func += "            fn(s, "
            count = 0
            for _type, _name, _parameter in i[2]:
                code_func += "_" + _name
                count = count + 1
                if count < len(i[2]):
                    code_func += ", "
            code_func += ")\n\n"

        else:
            raise Exception("func:%s wrong rpc type:%s must ntf" % (func_name, str(i[1])))

    code_constructor_end = "\n"
    code = "\n"
        
    return code_constructor + code_constructor_end + code_func + code