/*
 * conn_msg_handle.ts
 * qianqians
 * 2023/10/5
 */
import { decode } from '../@msgpack/msgpack'
import * as app2 from './app'

export class conn_msg_handle {
    public on_create_remote_entity(entity_type:string, entity_id:string, argvs:Uint8Array) {
        let info = decode<object>(argvs);
        if (typeof info === "object" && info) {
            app2.app.instance.create_entity(entity_type, entity_id, info);
        }
    }
    
    public on_delete_remote_entity(entity_id:string) {
        app2.app.instance.delete_entity(entity_id);
    }
    
    public on_refresh_entity(entity_type:string, entity_id:string, argvs:Uint8Array) {
        let info = decode<object>(argvs);
        if (typeof info === "object" && info) {
            app2.app.instance.update_entity(entity_type, entity_id, info);
        }
    }

    public on_kick_off(prompt_info:string) {
        app2.app.instance.on_kick_off(prompt_info);
    }
    
    public on_transfer_complete() {
        app2.app.instance.on_transfer_complete();
    }
    
    public on_call_rpc(hub_name:string, entity_id:string, msg_cb_id:number, method:string, argvs:Uint8Array) {
        let _player = app2.app.instance.player_mgr.get_player(entity_id);
        if (_player) {
            _player.handle_hub_request(method, hub_name, msg_cb_id, argvs);
        }
    }
    
    public on_call_rsp(entity_id:string, msg_cb_id:number, argvs:Uint8Array) {
        let _player = app2.app.instance.player_mgr.get_player(entity_id);
        if (_player) {
            _player.handle_hub_response(msg_cb_id, argvs);
            return;
        }

        let _subentity = app2.app.instance.subentity_mgr.get_subentity(entity_id);
        if (_subentity) {
            _subentity.handle_hub_response(msg_cb_id, argvs);
            return;
        }
    }
    
    public on_call_err(entity_id:string, msg_cb_id:number, argvs:Uint8Array) {
        let _player = app2.app.instance.player_mgr.get_player(entity_id);
        if (_player) {
            _player.handle_hub_response_error(msg_cb_id, argvs);
            return;
        }

        let _subentity = app2.app.instance.subentity_mgr.get_subentity(entity_id);
        if (_subentity) {
            _subentity.handle_hub_response_error(msg_cb_id, argvs);
            return;
        }
    }
    
    public on_call_ntf(hub_name:string, entity_id:string, method:string, argvs:Uint8Array) {
        let _player = app2.app.instance.player_mgr.get_player(entity_id);
        if (_player) {
            _player.handle_hub_notify(method, hub_name, argvs);
            return;
        }

        let _subentity = app2.app.instance.subentity_mgr.get_subentity(entity_id);
        if (_subentity) {
            _subentity.handle_hub_notify(method, hub_name, argvs);
            return;
        }

        let _receiver = app2.app.instance.receiver_mgr.get_receiver(entity_id)
        if (_receiver) {
            _receiver.handle_hub_notify(method, hub_name, argvs);
            return
        }
    }
    
    public on_call_global(method:string, hub_name:string, argvs:Uint8Array) {
        app2.app.instance.on_call_global(method, hub_name, argvs);
    }
}