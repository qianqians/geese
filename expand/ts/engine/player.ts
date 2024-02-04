/*
 * player.ts
 * qianqians
 * 2023/10/5
 */
import * as Base from './base_entity'
import * as CallBack from './callback'
import * as app from './app'

export abstract class player extends Base.base_entity {
    public request_msg_cb_id:number;
    public hub_request_callback:Map<string, (source:string, id:number, data:Uint8Array) => void>;
    public hub_notify_callback:Map<string, (source:string, data:Uint8Array) => void>;
    public hub_callback:Map<number, CallBack.callback>;

    public constructor(entity_type:string, entity_id:string) {
        super(entity_type, entity_id);

        this.request_msg_cb_id = Math.floor(Math.random() * 10011);
        this.hub_request_callback = new Map<string, (source:string, id:number, data:Uint8Array) => void>();
        this.hub_notify_callback = new Map<string, (source:string, data:Uint8Array) => void>();

        this.hub_callback = new Map<number, CallBack.callback>();

        app.app.instance.player_mgr.add_player(this);
    }

    abstract update_player(argvs: object): void; 

    public handle_hub_request(method:string, hub_name:string, msg_cb_id:number, argvs:Uint8Array) {
        let _call_handle = this.hub_request_callback.get(method);
        if (_call_handle) {
            _call_handle.call(null, hub_name, msg_cb_id, argvs)
        }
    }
    
    public del_callback(msg_cb_id:number) : boolean {
        return this.hub_callback.delete(msg_cb_id);
    }
    
    public handle_hub_response(msg_cb_id:number, argvs:Uint8Array) {
        let _call_handle = this.hub_callback.get(msg_cb_id);
        if (_call_handle) {
            _call_handle._callback.call(null, argvs);
            this.hub_callback.delete(msg_cb_id);
        }
    }
    
    public handle_hub_response_error(msg_cb_id:number, argvs:Uint8Array) {
        let _call_handle = this.hub_callback.get(msg_cb_id);
        if (_call_handle) {
            _call_handle._error.call(null, argvs);
            this.hub_callback.delete(msg_cb_id);
        }
    }
    
    public handle_hub_notify(method:string, hub_name:string, argvs:Uint8Array) {
        let _call_handle = this.hub_notify_callback.get(method);
        if (_call_handle) {
            _call_handle.call(null, hub_name, argvs);
        }
    }
    
    public reg_hub_request_callback(method:string, callback:(hub_name:string, msg_cb_id:number, argvs:Uint8Array) => void) {
        this.hub_request_callback.set(method, callback);
    }
    
    public reg_hub_notify_callback(method:string, callback:(hub_name:string, argvs:Uint8Array) => void) {
        this.hub_notify_callback.set(method, callback);
    }

    public call_hub_request(method:string, argvs:Uint8Array) : number {
        let msg_cb_id = this.request_msg_cb_id
        this.request_msg_cb_id += 1
        app.app.instance.ctx.call_rpc(this.EntityID, msg_cb_id, method, argvs);
        return msg_cb_id
    }
    
    public reg_hub_callback(msg_cb_id:number, rsp:CallBack.callback) {
        this.hub_callback.set(msg_cb_id, rsp)
    }
        
    public call_hub_response(msg_cb_id:number, argvs:Uint8Array) {
        app.app.instance.ctx.call_rsp(this.EntityID, msg_cb_id, argvs);
    }
    
    public call_hub_response_error(msg_cb_id:number, argvs:Uint8Array) {
        app.app.instance.ctx.call_err(this.EntityID, msg_cb_id, argvs);
    }
    
    public call_hub_notify(method:string, argvs:Uint8Array) {
        app.app.instance.ctx.call_ntf(this.EntityID, method, argvs);
    }
}

export class player_manager {
    public players:Map<string, player>;

    public constructor() {
        this.players = new Map<string, player>();
    }

    public add_player(_player:player) {
        this.players[_player.EntityID] = _player;
    }
    
    public update_player(entity_id:string, argvs: object) {
        let _player = this.get_player(entity_id);
        _player?.update_player(argvs);
    }

    public get_player(entity_id:string) : player | undefined {
        return this.players.get(entity_id);
    }

    public del_player(entity_id:string) {
        this.players.delete(entity_id);
    }
}