# 2018-3-16
# build by qianqians
# genjs

import sys

import uuid
import os
from parser import jparser

def gen_python_import_c(_import):
    code = "from threading import Timer\n"
    code += "from collections.abc import Callable\n"
    code += "from enum import Enum\n"
    code += "from .engine import *\n"
    code += "from .engine.msgpack import *\n"
    for _i in _import:
        code += "from ." + _i + "_cli import *\n"
    code += "\n"
    return code

def gen_python_import_h(_import):
    code = "from threading import Timer\n"
    code += "from collections.abc import Callable\n"
    code += "from enum import Enum\n"
    code += "from .engine import *\n"
    code += "from .engine.msgpack import *\n"
    for _i in _import:
        code += "from ." + _i + "_svr import *\n"
    code += "\n"
    return code

def gen_ts_import(_import):
    code = "import * as engine from \"./engine\";\n"
    code += "import { encode, decode } from \"./engine/@msgpack/msgpack\";\n"
    for _i in _import:
        code += "import * as " + _i + " from \"./" + _i + "_cli\";\n"
    return code

def gen(lang, inputdir, commondir, clioutputdir, svroutputdir):
    print("inputdir", inputdir)
    print("commondir", commondir)

    if clioutputdir != None and len(clioutputdir) > 0 and not os.path.isdir(clioutputdir):
        os.mkdir(clioutputdir)
    if svroutputdir != None and len(svroutputdir) > 0 and not os.path.isdir(svroutputdir):
        os.mkdir(svroutputdir)

    pretreatmentdata = jparser.batch(inputdir, commondir)
    for pretreatment in pretreatmentdata:
        if lang == 'python':
            from gen.common.python import genenum
            from gen.common.python import genstruct
            from gen.hub_call_client.python import gen
    
            c_code = gen_python_import_c(pretreatment._import)
            c_code += genenum.genenum(pretreatment)
            c_code += genstruct.genstruct(pretreatment)
            c_code += gen.genmodule(pretreatment) + "\n\n"

            s_code = gen_python_import_h(pretreatment._import)
            s_code += genenum.genenum(pretreatment)
            s_code += genstruct.genstruct(pretreatment)
            s_code += gen.gencaller(pretreatment) + "\n\n"

            if clioutputdir != None and len(clioutputdir) > 0:
                file = open(clioutputdir + '//' + pretreatment.name + "_cli.py", 'w')
                file.write(c_code)
                file.close()

            if svroutputdir != None and len(svroutputdir) > 0:
                file = open(svroutputdir + '//' + pretreatment.name + "_svr.py", 'w')
                file.write(s_code)
                file.close()
            
        elif lang == 'ts':
            from gen.common.ts import genenum
            from gen.common.ts import genstruct
            from gen.hub_call_client.ts import gen
            
            s_code = gen_ts_import(pretreatment._import)
            s_code += genenum.genenum(pretreatment)
            s_code += genstruct.genstruct(pretreatment)
            s_code += gen.genmodule(pretreatment) + "\n\n"
            
            if clioutputdir != None and len(clioutputdir) > 0:
                file = open(clioutputdir + '//' + pretreatment.name + "_cli.ts", 'w')
                file.write(s_code)
                file.close()

if __name__ == '__main__':
    if sys.argv[1] == "python":
        if len(sys.argv) == 6:
            gen(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5])
        else:
            gen(sys.argv[1], sys.argv[2], None, sys.argv[3], sys.argv[4])
    elif sys.argv[1] == "ts":
        if len(sys.argv) == 5:
            gen(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4], None)
        else:
            gen(sys.argv[1], sys.argv[2], None, sys.argv[3], None)