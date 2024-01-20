#coding:utf-8
# 2023-9-17
# build by qianqians
# gencaller

from .genentitycaller import gen_entity_caller
from .genmutilcaller import gen_mutil_caller
from .genglobalcaller import gen_global_caller

def gencaller(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    modules = pretreatment.module
    
    code = "#this caller code is codegen by geese codegen for python\n"
    for module_name, (_type, funcs) in modules.items():
        if _type == "entity_service":
            code += gen_entity_caller(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "mutil_service":
            code += gen_mutil_caller(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "global_service":
            code += gen_global_caller(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        
    return code

from .genentitymodule import gen_entity_module
from .genmutilmodule import gen_mutil_module
from .genglobalmodule import gen_global_module

def genmodule(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    modules = pretreatment.module
        
    code = "#this module code is codegen by geese codegen for python\n"
    for module_name, (_type, funcs) in modules.items():
        if _type == "entity_service":
            code += gen_entity_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "mutil_service":
            code += gen_mutil_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)
        elif _type == "global_service":
            code += gen_global_module(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)

    return code