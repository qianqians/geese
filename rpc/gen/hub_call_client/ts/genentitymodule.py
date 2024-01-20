#coding:utf-8
# 2020-1-21
# build by qianqians
# genmodule

import uuid
from tools.ts.tools import *
from tools.ts.gen_tools import *

def gen_entity_module(module_name, funcs, dependent_struct, dependent_enum, enum):
    code_declaration = "export class " + module_name + "_module {\n"
    code_declaration += "    public entity:engine.player;\n"
    code_constructor = "    public constructor(entity:engine.player) {\n"
    code_constructor += "        this.entity = entity\n\n"
        
    rsp_code = ""
    code_func = ""
    for i in funcs:
        func_name = i[0]
        if i[1] == "ntf":
            func_type = "(engine.session, "
            count = 0
            for _type, _name, _parameter in i[2]:
                func_type += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[2]):
                    func_type += ", "
            func_type += ") => void"
            
            code_declaration += "    public on_" + func_name + ":" + func_type + "[] = []\n"
            code_constructor += "        this.entity.reg_hub_notify_callback(\"" + func_name + "\", this." + func_name + ")\n"

            code_func += "    public " + func_name + "(hub_name:string, bin:Uint8Array):\n"
            code_func += "        inArray = decode(bin) as any;\n"
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
            code_func += "        s = new engine.session(hub_name)\n"
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
        elif i[1] == "req" and i[3] == "rsp" and i[5] == "err":
            func_type = "(" + module_name + "_" + func_name + "_rsp, "
            count = 0
            for _type, _name, _parameter in i[2]:
                func_type += convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[2]):
                    func_type += ", "
            func_type += ") => void"
            
            code_declaration += "    public on_" + func_name + ":(" + func_type + ")[] = []\n"
            code_constructor += "        this.entity.reg_hub_request_callback(\"" + func_name + "\", this." + func_name + ")\n"
            
            code_func += "    public " + func_name + "(hub_name:string, msg_cb_id:number, bin:Uint8Array) {\n"
            code_func += "        let inArray = decode(bin) as any;\n"
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
            code_func += "        let rsp = new " + module_name + "_" + func_name + "_rsp(hub_name, msg_cb_id, this.entity)\n"
            code_func += "        for (let fn of this.on_" + func_name + ") {\n"
            code_func += "            fn(rsp, "
            count = 0
            for _type, _name, _parameter in i[2]:
                code_func += "_" + _name
                count = count + 1
                if count < len(i[2]):
                    code_func += ", "
            code_func += ")\n;\n"
            code_func += "        }\n"
            code_func += "    }\n\n"

            rsp_code += "export class " + module_name + "_" + func_name + "_rsp extends engine.session {\n"
            rsp_code += "    public entity:engine.player;\n"
            rsp_code += "    public is_rsp:boolean = false;\n"
            rsp_code += "    public msg_cb_id:number;\n"
            rsp_code += "    public constructor(hub_name:string, msg_cb_id:number, entity:engine.player) {\n"
            rsp_code += "        super(hub_name);\n"
            rsp_code += "        this.entity = entity;\n"
            rsp_code += "        this.msg_cb_id = msg_cb_id;\n"
            rsp_code += "    }\n\n"

            rsp_code += "    public rsp("
            count = 0
            for _type, _name, _parameter in i[4]:
                if _parameter == None:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[4]):
                    rsp_code += ", "
            rsp_code += ") {\n"
            rsp_code += "        if (this.is_rsp) {\n"
            rsp_code += "            return\n"
            rsp_code += "        }\n"
            rsp_code += "        this.is_rsp = false;\n\n"
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            rsp_code += "        let _argv_" + _argv_uuid + ":any[] = [];\n"
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
            rsp_code += "        this.entity.call_hub_response(this.msg_cb_id, encode(_argv_" + _argv_uuid + "));\n"
            rsp_code += "    }\n\n"

            rsp_code += "    public err("
            count = 0
            for _type, _name, _parameter in i[6]:
                if _parameter == None:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    rsp_code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[6]):
                    rsp_code += ", "
            rsp_code += ") {\n"
            rsp_code += "        if (this.is_rsp) {\n"
            rsp_code += "            return\n"
            rsp_code += "        }\n"
            rsp_code += "        this.is_rsp = false;\n\n"
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))
            rsp_code += "        let _argv_" + _argv_uuid + ":any[] = [];\n"
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
            rsp_code += "        this.entity.call_hub_response_error(this.msg_cb_id, encode(_argv_" + _argv_uuid + "));\n"
            rsp_code += "    }\n\n"
            rsp_code += "}\n\n"

        else:
            raise Exception("func:%s wrong rpc type:%s must req or ntf" % (func_name, str(i[1])))

    code_constructor_end = "    }\n\n"
    code = "}\n"
        
    return rsp_code + code_declaration + code_constructor + code_constructor_end + code_func + code