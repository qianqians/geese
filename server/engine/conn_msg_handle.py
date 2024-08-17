# -*- coding: UTF-8 -*-
from collections.abc import Callable
from .msgpack import *

class conn_msg_handle(object):
    def on_client_request_login(self, gate_name:str, conn_id:str, sdk_uuid:str):
        from app import app
        app().run_coroutine_async(app().login_handle.login(gate_name, conn_id, sdk_uuid))

    def on_client_request_reconnect(self, gate_name:str, conn_id:str, entity_id:str, token:str):
        from app import app
        app().run_coroutine_async(app().login_handle.reconnect(gate_name, conn_id, entity_id, token))
    
    def on_transfer_entity_control(self, entity_id:str, is_main:bool, is_replace:bool, gate_name:str, conn_id:str):
        from app import app
        app().login_handle.on_transfer_entity_control(entity_id, is_main, is_replace, gate_name, conn_id)
        
    def on_client_disconnnect(self, gate_name:str, conn_id:str):
        from app import app
        app().player_mgr.player_offline(conn_id)
        
    def on_kick_off_client(self, gate_name:str, conn_id:str):
        from app import app
        app().player_mgr.player_offline(conn_id)
        app().ctx.hub_call_kick_off_client_complete(gate_name, conn_id)
        
    def on_client_request_service(self, service_name:str, gate_name:str, conn_id:str):
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity(gate_name, conn_id)
        
    def on_client_call_rpc(self, gate_name:str, conn_id:str, entity_id:str, msg_cb_id:int, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_client_request(gate_name, conn_id, method, msg_cb_id, argvs)
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.handle_client_request(gate_name, conn_id, method, msg_cb_id, argvs)
            return
        app().error("unhandle client request method:{} entity:{}, ".format(method, entity_id))
        
    def on_client_call_rsp(self, gate_name:str, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_client_response(gate_name, msg_cb_id, argvs)
            return
        app().error("unhandle client response msg_cb_id:{} entity:{}, ".format(msg_cb_id, entity_id))
        
    def on_client_call_err(self, gate_name:str, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_client_response_error(gate_name, msg_cb_id, argvs)
            return
        app().error("unhandle client response error msg_cb_id:{} entity:{}, ".format(msg_cb_id, entity_id))
        
    def on_client_call_ntf(self, gate_name:str, entity_id:str, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_client_notify(gate_name, method, argvs)
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.handle_client_notify(gate_name, method, argvs)
            return
        app().error("unhandle client notify method:{} entity:{}, ".format(method, entity_id))

    def on_rge_hub(self, hub_name):
        from app import app
        app().trace(f"on_rge_hub hub_name:{hub_name}")

    def on_query_service_entity(self, hub_name:str, service_name:str):
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.hub_query_service_entity(hub_name)

    def on_create_service_entity(self, source_hub_name:str, service_name:str, entity_id:str, entity_type:str, argvs:bytes):
        from app import app
        app().create_entity(entity_type, source_hub_name, entity_id, loads(argvs))

    def on_forward_client_request_service(self, hub_name:str, service_name:str, gate_name:str, conn_id:str):
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity(gate_name, conn_id)

    def on_call_hub_rpc(self, source_hub_name:str, entity_id:str, msg_cb_id:int, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_request(source_hub_name, method, msg_cb_id, argvs)
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.handle_hub_request(source_hub_name, method, msg_cb_id, argvs)
            return
        app().error("unhandle hub request msg_cb_id:{} entity:{}, ".format(msg_cb_id, entity_id))

    def on_call_hub_rsp(self, hub_name:str, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_response(hub_name, msg_cb_id, argvs)
            return
        app().error("unhandle hub response msg_cb_id:{} entity:{}, ".format(msg_cb_id, entity_id))

    def on_call_hub_err(self, hub_name:str, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_response_error(hub_name, msg_cb_id, argvs)
            return
        app().error("unhandle hub response error msg_cb_id:{} entity:{}, ".format(msg_cb_id, entity_id))

    def on_call_hub_ntf(self, source_hub_name:str, entity_id:str, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_notify(source_hub_name, method, argvs)
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.handle_hub_notify(source_hub_name, method, argvs)
            return
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_notify(source_hub_name, method, argvs)
            return
        _receiver = app().receiver_mgr.get_receiver(entity_id)
        if _receiver != None:
            _receiver.handle_hub_notify(source_hub_name, method, argvs)
            return
        app().error("unhandle hub request method:{} entity:{}, ".format(method, entity_id))
        
    def on_wait_migrate_entity(self, hub_name:str, entity_id:str):
        from app import app
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.is_migrate = True
            app().ctx.hub_call_response_migrate_entity(hub_name, entity_id)
        else:
            app().error("unhandle hub on_wait_migrate_entity entity:{}, ".format(entity_id))
            
    def on_migrate_entity(self, hub_name:str, entity_type:str, entity_id:str, gates:list[str], hubs:list[str], argvs:bytes):
        from app import app
        app().create_migrate_entity(entity_type, entity_id, loads(argvs))
        
        
    def on_migrate_entity_complete(self, hub_name:str, entity_id:str):
        from app import app
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.do_cache_msg()
        else:
            app().error("unhandle hub on_migrate_entity_complete entity:{}, ".format(entity_id))
            
    def on_response_migrate_entity(self, svr_name:str, entity_id:str):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.check_migrate_entity_lock(svr_name)
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.check_migrate_entity_lock(svr_name)
            return
        app().error("unhandle on_response_migrate_entity entity:{}, ".format(entity_id))