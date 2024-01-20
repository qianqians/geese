# -*- coding: UTF-8 -*-
from collections.abc import Callable
from .msgpack import *

class conn_msg_handle(object):
    def on_create_remote_entity(self, entity_type:str, entity_id:str, argvs:bytes):
        from app import app
        app().create_entity(entity_type, entity_id, loads(argvs))

    def on_delete_remote_entity(self, entity_id:str):
        from app import app
        app().delete_entity(entity_id)
        
    def on_refresh_entity(self, entity_type:str, entity_id:str, argvs:bytes):
        from app import app
        app().update_entity(entity_type, entity_id, loads(argvs))
        
    def on_kick_off(self, prompt_info:str):
        from app import app
        app().close()
        app().on_kick_off(prompt_info)

    def on_transfer_complete(self):
        from app import app
        app().on_transfer_complete()
        
    def on_call_rpc(self, hub_name:str, entity_id:str, msg_cb_id:int, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_request(method, hub_name, msg_cb_id, argvs)
            return
        print("unhandle hub request method:{} entity:{}".format(method, entity_id))

    def on_call_rsp(self, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_response(msg_cb_id, argvs)
            return
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_response(msg_cb_id, argvs)
            return
        print("unhandle hub response msg_cb_id:{} entity:{}".format(msg_cb_id, entity_id))

    def on_call_err(self, entity_id:str, msg_cb_id:int, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_response_error(msg_cb_id, argvs)
            return
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_response_error(msg_cb_id, argvs)
            return
        print("unhandle hub response error msg_cb_id:{} entity:{}".format(msg_cb_id, entity_id))

    def on_call_ntf(self, hub_name:str, entity_id:str, method:str, argvs:bytes):
        from app import app
        _player = app().player_mgr.get_player(entity_id)
        if _player != None:
            _player.handle_hub_notify(method, hub_name, argvs)
            return
        _subentity = app().subentity_mgr.get_subentity(entity_id)
        if _subentity != None:
            _subentity.handle_hub_notify(method, hub_name, argvs)
            return
        _receiver = app().receiver_mgr.get_receiver(entity_id)
        if _receiver != None:
            _receiver.handle_hub_notify(method, hub_name, argvs)
            return
        print("unhandle hub response notify msg_cb_id:{} entity:{}".format(method, entity_id))
        
    def on_call_global(self, method:str, argvs:bytes):
        from app import app
        app().on_call_global(method, argvs)