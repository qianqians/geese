# 2018-3-16
# build by qianqians
# genjs

import sys

import uuid
import os
from parser import jparser

def gen_python_import(_import):
    code = "from threading import Timer\n"
    code += "from collections.abc import Callable\n"
    code += "from enum import Enum\n"
    code += "from .engine import *\n"
    for _i in _import:
        code += "from ." + _i + "_svr import *\n"
    code += "\n"
    return code

def gen(lang, inputdir, commondir, outputdir):
    print("inputdir", inputdir)
    print("commondir", commondir)

    if not os.path.isdir(outputdir):
        os.mkdir(outputdir)

    pretreatmentdata = jparser.batch(inputdir, commondir)
    for pretreatment in pretreatmentdata:
        if lang == 'python':
            from gen.common.python import genenum
            from gen.common.python import genstruct
            from gen.hub_call_hub.python import gen
            
            code = gen_python_import(pretreatment._import)
            code += genenum.genenum(pretreatment)
            code += genstruct.genstruct(pretreatment)
            code += gen.gencaller(pretreatment)
            code += gen.genmodule(pretreatment)
            code += "\n"

            file = open(outputdir + '//' + pretreatment.name + "_svr.py", 'w')
            file.write(code)
            file.close()

if __name__ == '__main__':
    if len(sys.argv) == 5:
        gen(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        gen(sys.argv[1], None, sys.argv[2], sys.argv[3])