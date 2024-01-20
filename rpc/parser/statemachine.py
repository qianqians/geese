#coding:utf-8
# 2014-12-18
# build by qianqians
# statemachine

from .deletenonespacelstrip import deleteNoneSpacelstrip
from .modulemachine import module
from .enummachine import enum
from .structmachine import struct
from .importmachine import _import

class statemachine(object):
    def __init__(self):
        self.keyworld = ''
        self.module:dict = {}
        self.enum:dict = {}
        self.struct:dict = {}
        self._import:list = []
        self.machine = None

    def push(self, ch):
        if self.machine is not None:
            if self.machine.push(ch):
                if isinstance(self.machine, module):
                    self.module[self.machine.name] = (self.machine.type, self.machine.module)
                    self.machine = None
                if isinstance(self.machine, enum):
                    self.enum[self.machine.name] = self.machine.enum
                    self.machine = None
                if isinstance(self.machine, struct):
                    self.struct[self.machine.name] = self.machine.elem
                    self.machine = None
                if isinstance(self.machine, _import):
                    self._import.append(self.machine.name)
                    self.machine = None
        else:
            if ch in [' ', '    ', '\r', '\n', '\t', '\0', '\r\n']:
                self.keyworld = deleteNoneSpacelstrip(self.keyworld)
                if 'service' in self.keyworld:
                    self.machine = module(self.keyworld)
                    self.keyworld = ''
                elif self.keyworld == 'enum':
                    self.machine = enum()
                    self.keyworld = ''
                elif self.keyworld == 'struct':
                    self.machine = struct()
                    self.keyworld = ''
                elif self.keyworld == 'import':
                    self.machine = _import()
                    self.keyworld = ''
            else:
                self.keyworld += ch

    def getmodule(self):
        print("module:" + str(self.module))
        return self.module

    def getenum(self):
        print("enum:" + str(self.enum))
        return self.enum

    def getstruct(self):
        print("struct" + str(self.struct))
        return self.struct

    def getimport(self):
        print("import" + str(self._import))
        return self._import

    def syntaxanalysis(self, genfilestr):
        for str in genfilestr:
            for ch in str:
                self.push(ch)
