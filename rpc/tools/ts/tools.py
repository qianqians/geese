#coding:utf-8
# 2019-12-26
# build by qianqians
# tools

class TypeType():
    Enum = 0
    Custom = 1
    String = 2
    Int8 = 3
    Int16 = 4
    Int32 = 5
    Int64 = 6
    Uint8 = 7
    Uint16 = 8
    Uint32 = 9
    Uint64 = 10
    Float = 11
    Double = 12
    Bool = 13
    Bin = 14
    List = 15
    Dict = 16

def check_in_dependent(typestr, dependent):
    for _type, _import in dependent:
        if _type == typestr:
            return True
    return False

def get_import(typestr, dependent):
    for _type, _import in dependent:
        if _type == typestr:
            return _import
    return ""

def check_type(typestr, dependent_struct, dependent_enum):
    if typestr == 'int8':
        return TypeType.Int8
    elif typestr == 'int16':
        return TypeType.Int16
    elif typestr == 'int32':
        return TypeType.Int32
    elif typestr == 'int64':
        return TypeType.Int64
    elif typestr == 'uint8':
        return TypeType.Uint8
    elif typestr == 'uint16':
        return TypeType.Uint16
    elif typestr == 'uint32':
        return TypeType.Uint32
    elif typestr == 'uint64':
        return TypeType.Uint64
    elif typestr == 'string':
        return TypeType.String
    elif typestr == 'float':
        return TypeType.Float
    elif typestr == 'double':
        return TypeType.Double
    elif typestr == 'bool':
        return TypeType.Bool
    elif typestr == 'bin':
        return TypeType.Bin
    elif check_in_dependent(typestr, dependent_struct):
        return TypeType.Custom
    elif check_in_dependent(typestr, dependent_enum):
        return TypeType.Enum
    elif typestr[0:4] == 'list' and typestr[4] == '<' and typestr[-1] == '>':
        return TypeType.List
    elif typestr[0:3] == 'map' and typestr[3] == '<' and typestr[-1] == '>':
        return TypeType.Dict

    raise Exception("non exist type:%s" % typestr)

def convert_parameter(typestr, parameter, dependent_enum, enum):
    if typestr == 'int8':
        return parameter
    elif typestr == 'int16':
        return parameter
    elif typestr == 'int32':
        return parameter
    elif typestr == 'int64':
        return parameter
    elif typestr == 'uint8':
        return parameter
    elif typestr == 'uint16':
        return parameter
    elif typestr == 'uint32':
        return parameter
    elif typestr == 'uint64':
        return parameter
    elif typestr == 'string':
        return parameter
    elif typestr == 'float':
        return parameter
    elif typestr == 'double':
        return parameter
    elif typestr == 'bool':
        return parameter
    elif check_in_dependent(typestr, dependent_enum):
        _type = parameter.split(".")[0]
        _parameter = parameter.split(".")[1]
        enum_elems = enum[typestr]
        for key, value in enum_elems:
            if key == _parameter and _type == typestr:
                _import = get_import(typestr, dependent_enum)
                if _import == "":
                    return typestr + '.' + parameter
                else:
                    return _import + '.' + typestr + '.' + parameter
        raise Exception("parameter:%s not %s member" % (parameter, typestr))
    elif typestr == 'bin':
        str_parameter = "Uint8Array.from(%s)"%parameter
        return str_parameter

def default_parameter(typestr, dependent_struct, dependent_enum, enum):
    if typestr == 'int8':
        return '0'
    elif typestr == 'int16':
        return '0'
    elif typestr == 'int32':
        return '0'
    elif typestr == 'int64':
        return '0'
    elif typestr == 'uint8':
        return '0'
    elif typestr == 'uint16':
        return '0'
    elif typestr == 'uint32':
        return '0'
    elif typestr == 'uint64':
        return '0'
    elif typestr == 'string':
        return '\"\"'
    elif typestr == 'float':
        return '0.0'
    elif typestr == 'double':
        return '0.0'
    elif typestr == 'bool':
        return 'false'
    elif check_in_dependent(typestr, dependent_enum):
        _import = get_import(typestr, dependent_enum)
        enum_elems = enum[typestr]
        key, _ = enum_elems[0]
        if _import == "":
            return typestr + '.' + key
        else:
            return _import + '.' + typestr + '.' + key
    elif typestr == 'bin':
        str_parameter = "Uint8Array.from([])"
        return str_parameter
    elif check_in_dependent(typestr, dependent_struct):
        return 'null'
    elif typestr[0:4] == 'list' and typestr[4] == '<' and typestr[-1] == '>':
        return '[]'
    elif typestr[0:3] == 'map' and typestr[3] == '<' and typestr[-1] == '>':
        return 'new Map()'
    
    return 'null'

def convert_type(typestr, dependent_struct, dependent_enum):
    if typestr == 'int8':
        return 'number'
    elif typestr == 'int16':
        return 'number'
    elif typestr == 'int32':
        return 'number'
    elif typestr == 'int64':
        return 'number'
    elif typestr == 'uint8':
        return 'number'
    elif typestr == 'uint16':
        return 'number'
    elif typestr == 'uint32':
        return 'number'
    elif typestr == 'uint64':
        return 'number'
    elif typestr == 'string':
        return 'string'
    elif typestr == 'float':
        return 'number'
    elif typestr == 'double':
        return 'number'
    elif typestr == 'bool':
        return 'boolean'
    elif typestr == 'bin':
        return 'Uint8Array'
    elif check_in_dependent(typestr, dependent_struct):
        _import = get_import(typestr, dependent_struct)
        if _import == "":
            return typestr
        else:
            return _import + "." + typestr
    elif check_in_dependent(typestr, dependent_enum):
        _import = get_import(typestr, dependent_enum)
        if _import == "":
            return typestr
        else:
            return _import + "." + typestr
    elif typestr[0:4] == 'list' and typestr[4] == '<' and typestr[-1] == '>':
        array_type = typestr[5:-1]
        array_type = convert_type(array_type, dependent_struct, dependent_enum)
        return 'Array<' + array_type +'>'
    elif typestr[0:3] == 'map' and typestr[3] == '<' and typestr[-1] == '>':
        array_type = typestr[4:-1]
        array_type = convert_type(array_type, dependent_struct, dependent_enum)
        return 'Map<' + array_type + '>'
    
    raise Exception("non exist type:%s" % typestr)
    
def check_type_original(_type:int):
    OriginalTypeList = [
        TypeType.Enum, TypeType.String, TypeType.Int8, TypeType.Int16, TypeType.Int32, TypeType.Int64,
        TypeType.Uint8, TypeType.Uint16, TypeType.Uint32, TypeType.Uint64, 
        TypeType.Float, TypeType.Double, TypeType.Bool, TypeType.Bin]
    
    return _type in OriginalTypeList