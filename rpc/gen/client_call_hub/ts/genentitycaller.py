#coding:utf-8
# 2020-1-21
# build by qianqians
# gencaller

import uuid
from tools.ts.tools import *
from tools.ts.gen_tools import *

def gen_entity_caller(module_name, funcs, dependent_struct, dependent_enum, enum):
    code = "export class " + module_name + "_caller {\n"
    code += "    public entity:engine.subentity|engine.player;\n"
    code += "    public constructor(entity:engine.subentity|engine.player) {\n"
    code += "        this.entity = entity;\n"
    code += "    }\n\n"
    
    cb_code = ""

    for i in funcs:
        func_name = i[0]

        if i[1] == "ntf":
            code += "    public  " + func_name + "("
            count = 0
            for _type, _name, _parameter in i[2]:
                if _parameter == None:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[2]):
                    code += ", "
            code += ") {\n"
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
            code += "        this.entity.call_hub_notify(\"" + func_name + "\", encode(_argv_" + _argv_uuid + "))\n"
            code += "    }\n\n"
        elif i[1] == "req" and i[3] == "rsp" and i[5] == "err":
            rsp_fn = "("
            count = 0
            for _type, _name, _parameter in i[4]:
                rsp_fn += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[4]):
                    rsp_fn += ", "
            rsp_fn += ") => void"
            
            err_fn = "("
            count = 0
            for _type, _name, _parameter in i[6]:
                err_fn += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                count += 1
                if count < len(i[6]):
                    err_fn += ", "
            err_fn += ") => void"

            cb_code += "export class " + module_name + "_" + func_name + "_cb {\n"
            cb_code += "    public entity:engine.subentity|engine.player;\n"
            cb_code += "    public cb:(" + rsp_fn + ")|null = null;\n"
            cb_code += "    public err:(" + err_fn + ")|null = null;\n"
            cb_code += "    public rsp:engine.callback;\n"
            cb_code += "    public constructor(_cb_uuid:number, _entity:engine.subentity|engine.player) {\n"
            cb_code += "        this.entity = _entity\n"
            cb_code += "        this.rsp = new engine.callback(() => { return this.entity.del_callback(_cb_uuid); });\n"
            cb_code += "        this.entity.reg_hub_callback(_cb_uuid, this.rsp)\n\n"
            cb_code += "    }\n\n"

            cb_code += "    private on_rsp(bin:Uint8Array) {\n"
            cb_code += "        let inArray = decode(bin) as any;\n"
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
            cb_code += "        if (this.cb) this.cb("
            count = 0
            for _type, _name, _parameter in i[4]:
                cb_code += "_" + _name
                count = count + 1
                if count < len(i[4]):
                    cb_code += ", "
            cb_code += ");\n\n"
            cb_code += "    }\n\n"

            cb_code += "    private on_err(bin:Uint8Array) {\n"
            cb_code += "        let inArray = decode(bin) as any;\n"
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
            cb_code += "        if (this.err) this.err("
            count = 0
            for _type, _name, _parameter in i[6]:
                cb_code += "_" + _name
                count = count + 1
                if count < len(i[4]):
                    cb_code += ", "
            cb_code += ")\n\n"
            cb_code += "    }\n\n"

            cb_code += "    public callBack(_cb:" + rsp_fn + ", _err:" + err_fn + ") {\n"
            cb_code += "        this.cb = _cb;\n"
            cb_code += "        this.err = _err;\n"
            cb_code += "        this.rsp.callback(this.on_rsp, this.on_err);\n"
            cb_code += "        return this.rsp;\n"
            cb_code += "    }\n\n"

            _cb_uuid_uuid = '_'.join(str(uuid.uuid5(uuid.NAMESPACE_DNS, func_name)).split('-'))
            _argv_uuid = '_'.join(str(uuid.uuid3(uuid.NAMESPACE_DNS, func_name)).split('-'))

            code += "    public  " + func_name + "("
            count = 0
            for _type, _name, _parameter in i[2]:
                if _parameter == None:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum)
                else:
                    code += _name + ":" + convert_type(_type, dependent_struct, dependent_enum) + " = " + convert_parameter(_type, _parameter, dependent_enum, enum)
                count = count + 1
                if count < len(i[2]):
                    code += ", "
            code += ") {\n"
            code += "        let _argv_" + _argv_uuid + ":any[] = []\n"
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
            code += "        let _cb_uuid = this.entity.call_hub_request(\"" + func_name + "\", encode(_argv_" + _argv_uuid + "));\n"
            code += "        return new " + module_name + "_" + func_name + "_cb(_cb_uuid, this.entity);\n"
            code += "    }\n\n"
            cb_code += "}\n\n"

        else:
            raise Exception("func:" + func_name + " wrong rpc type:" + str(i[1]) + ", must req or ntf")

    code += "}\n"
    
    return cb_code + code