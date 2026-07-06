# -*- coding: UTF-8 -*-
from app import singleton, app

@singleton
class global_entity(object):
    def call_client(self, method:str, argvs:bytes):
        for gate_name in self.conn_client_gate:
            app().ctx.hub_call_client_global(gate_name, method, argvs)