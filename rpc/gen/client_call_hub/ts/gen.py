#coding:utf-8
# 2023-9-17
# build by qianqians
# gencaller

from .genentitycaller import gen_entity_caller

def gencaller(pretreatment):
    dependent_struct = pretreatment.dependent_struct
    dependent_enum = pretreatment.dependent_enum
    
    modules = pretreatment.module
    
    code = "// this caller code is codegen by geese codegen for typescript\n"
    for module_name, (_type, funcs) in modules.items():
        if _type == "entity_service":
            code += gen_entity_caller(module_name, funcs, dependent_struct, dependent_enum, pretreatment.enum)

    return code