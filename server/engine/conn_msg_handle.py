# -*- coding: UTF-8 -*-
from collections.abc import Callable
import asyncio
from .msgpack import *

class conn_msg_handle(object):
    def on_client_request_login(self, gate_name:str, conn_id:str, sdk_uuid:str, argvs:bytes):
        from app import app
        app().run_coroutine_async(app().login_handle.login(gate_name, conn_id, sdk_uuid, loads(argvs)))

    def on_client_request_reconnect(self, gate_name:str, conn_id:str, entity_id:str, argvs:bytes):
        from app import app
        app().run_coroutine_async(app().login_handle.reconnect(gate_name, conn_id, entity_id, loads(argvs)))
    
    def do_transfer_msg_end(self, conn_id:str, is_kick_off:bool):
        from app import app
        _t = app().ctx.transfer_timeout[conn_id]
        if _t!= None:
            _t.cancel()
            app().ctx.transfer_timeout.pop(conn_id)

    def on_transfer_entity_control(self, entity_id:str, is_main:bool, is_reconnect:bool, gate_name:str, conn_id:str):
        from app import app
        is_entry_player = app().player_mgr.update_player_conn(entity_id, is_main, is_reconnect, gate_name, conn_id) 
        is_entry_entity = app().entity_mgr.update_entity_conn(entity_id, is_reconnect, gate_name, conn_id)
        if (not is_entry_player) and (not is_entry_entity) and is_reconnect:
            app().ctx.hub_call_client_delete_remote_entity(gate_name, entity_id)

    def on_client_disconnnect(self, gate_name:str, conn_id:str):
        from app import app
        app().player_mgr.player_offline(conn_id)
        
    def on_kick_off_client(self, gate_name:str, conn_id:str):
        from app import app
        app().player_mgr.player_offline(conn_id)
        app().ctx.hub_call_kick_off_client_complete(gate_name, conn_id)
        
    def on_client_request_service(self, service_name:str, gate_name:str, conn_id:str, argvs:bytes):
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity(gate_name, conn_id, loads(argvs))
        
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

    def on_create_service_entity(self, source_hub_name:str, is_migrate:bool, service_name:str, entity_id:str, entity_type:str, argvs:bytes):
        from app import app
        app().create_entity(is_migrate, entity_type, source_hub_name, entity_id, loads(argvs))

    def on_forward_client_request_service(self, hub_name:str, service_name:str, gate_name:str, conn_id:str, argvs:bytes):
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity(gate_name, conn_id, loads(argvs))

    def on_forward_client_request_service_ext(self, hub_name:str, service_name:str, info:list[(str, str, bytes)]):
        info_ext = []
        for _info in info:
            gate_name, conn_id, argvs = _info
            info_ext.append((gate_name, conn_id, loads(argvs)))
            
        from app import app
        _service = app().service_mgr.get_service(service_name)
        _service.client_query_service_entity_ext(info_ext)

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
        else:
            app().error("unhandle hub on_wait_migrate_entity entity:{}, ".format(entity_id))
            
    def on_migrate_entity(self, hub_name:str, entity_type:str, entity_id:str, main_gate_name:str, main_conn_id:str, gates:list[str], hubs:list[str], argvs:bytes):
        from app import app
        app().create_migrate_entity(entity_type, entity_id, main_gate_name, main_conn_id, gates, hubs, loads(argvs))
        
    def on_create_migrate_entity(self, svr_name:str, entity_id:str):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.migrate_entity_complete()
            return
        _entity = app().entity_mgr.get_entity(entity_id)
        if _entity != None:
            _entity.migrate_entity_complete()
            return
        app().error("unhandle on_response_migrate_entity entity:{}, ".format(entity_id))
        
    def on_migrate_entity_complete(self, hub_name:str, source_hub_name:str, entity_id:str):
        from app import app
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.do_cache_msg(source_hub_name)
        else:
            app().error("unhandle hub on_migrate_entity_complete entity:{}, ".format(entity_id))
            