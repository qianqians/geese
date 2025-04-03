#coding:utf-8
# 2023-9-17
# build by qianqians
# genmodule

import uuid
from tools.ts.tools import *
from tools.ts.gen_tools import *

def gen_mutil_module(module_name, funcs, dependent_struct, dependent_enum, enum):
    code_declaration = "export class " + module_name + "_module {\n"
    code_declaration += "    public entity:engine.player|engine.subentity|engine.receiver;\n"
    code_constructor = "    public constructor(entity:engine.player|engine.subentity|engine.receiver) {\n"
    code_constructor += "        this.entity = entity;\n"
        
    code_func = ""
    for i in funcs:
        func_name = i[0]
        if i[1] == "ntf":
            func_type = "((s:engine.session, "
            count = 0
            for _type, _name, _parameter in i[2]:
                func_type +=  _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[2]):
                    func_type += ", "
            func_type += ") => void)"
            
            code_declaration += "    public on_" + func_name + ":" + func_type + "[] = [];\n"
            code_constructor += "        this.entity.reg_hub_notify_callback(\"" + func_name + "\", this." + func_name + ");\n"

            code_func += "    public " + func_name + "(hub_name:string, bin:Uint8Array) {\n"
            code_func += "        let inArray = decode(bin) as any;\n"
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
            code_func += "        let s = new engine.session(hub_name)\n"
            code_func += "        for (let fn of this.on_" + func_name + ") {\n"
            code_func += "            fn(s, "
            count = 0
            for _type, _name, _parameter in i[2]:
                code_func += "_" + _name
                count = count + 1
                if count < len(i[2]):
                    code_func += ", "
            code_func += ");\n"
            code_func += "        }\n"
            code_func += "    }\n\n"

        else:
            raise Exception("func:%s wrong rpc type:%s must ntf" % (func_name, str(i[1])))

    code_constructor_end = "    }\n\n"
    code = "}\n"
        
    return code_declaration + code_constructor + code_constructor_end + code_func + code