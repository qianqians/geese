# -*- coding: UTF-8 -*-
from .pyclient import ClientContext

class context(object):
    def __init__(self) -> None:
        self.ctx = ClientContext()

    def connect_tcp(self, addr:str, port:int) -> bool:
        return self.ctx.connect_tcp(addr, port)
    
    def connect_ws(self, host:str) -> bool:
        return self.ctx.connect_ws(host)
    
    def login(self, sdk_uuid:str) -> bool:
        return self.ctx.login(sdk_uuid)
    
    def reconnect(self, account_id:str, token:str) -> bool:
        return self.ctx.reconnect(account_id, token)
    
    def request_hub_service(self, service_name:str) -> bool:
        return self.ctx.request_hub_service(service_name)
        
    def call_rpc(self, entity_id:str, msg_cb_id:int, method:str, argvs:bytes) -> bool:
        return self.ctx.call_rpc(entity_id, msg_cb_id, method, argvs)
    
    def call_rsp(self, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.call_rsp(entity_id, msg_cb_id, argvs)
    
    def call_err(self, entity_id:str, msg_cb_id:int, argvs:bytes) -> bool:
        return self.ctx.call_err(entity_id, msg_cb_id, argvs)
    
    def call_ntf(self, entity_id:str, method:str, argvs:bytes) -> bool:
        return self.ctx.call_ntf(entity_id, method, argvs)

    def poll_conn_msg(self, handle) -> bool:
        return self.ctx.poll_conn_msg(handle)