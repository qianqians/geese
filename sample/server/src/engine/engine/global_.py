# -*- coding: UTF-8 -*-
from app import singleton, app

@singleton
class global_entity(object):
    __instance__ = None
    
    def __init__(self):
        __instance__ = self
        
    def instance() -> app:
        return app.__instance__

    def call_client(self, method:str, argvs:bytes):
        for gate_name in self.conn_client_gate:
            app().ctx.hub_call_client_global(gate_name, method, argvs)