#coding:utf-8
# 2023-9-17
# build by qianqians
# gencaller

from .genentitymodule import gen_entity_module
from .genmutilmodule import gen_mutil_module
from .genglobalmodule import gen_global_module

def genmodule(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    modules = pretreatment.module
        
    code = "// this module code is codegen by geese codegen for typescript\n"
    for module_name, (_type, funcs) in modules.items():
        if _type == "entity_service":
            code += gen_entity_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "mutil_service":
            code += gen_mutil_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "global_service":
            code += gen_global_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)

    return code