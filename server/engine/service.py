# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod

def ServiceDescribe(service_name:str):
    def wrapper(cls):
        cls.service_name = service_name
        return cls
    return wrapper

class service(ABC):
    @abstractmethod
    def hub_query_service_entity(self, queryer_hub_name:str):
        pass
    
    @abstractmethod
    def client_query_service_entity(self, queryer_gate_name:str, queryer_client_conn_id:str):
        pass
    
class service_manager(object):
    def __init__(self):
        self.services:dict[str, service] = {}
        
    def reg_service(self, _service:service):
        self.services[_service.service_name] = _service
        
        from app import app
        app().register_service(_service.service_name)

    def get_service(self, service_name:str) -> service:
        return self.services[service_name]
    
async def query_service(service_name:str):
    from app import app
    hub_name = await app().ctx.entry_hub_service(service_name)
    if app().ctx.hub_name() == hub_name:
        _service = app().service_mgr.get_service(service_name)
        _service.hub_query_service_entity(hub_name)
    else:
        app().ctx.query_service(hub_name, service_name)
        
async def forward_client_query_service(service_name:str, gate_name:str, gate_host:str, conn_id:str):
    from app import app
    hub_name = await app().ctx.entry_hub_service(service_name)
    if app().ctx.hub_name() == hub_name:
        await app().ctx.entry_gate_service(gate_name, gate_host)
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity(gate_name, conn_id)
    else:
        app().ctx.forward_client_request_service(hub_name, service_name, gate_name, gate_host, conn_id)