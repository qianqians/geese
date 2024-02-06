/*
 * app.ts
 * qianqians
 * 2023/10/5
 */
import * as context from './context'
import * as ConnMsgHandle from './conn_msg_handle'
import * as player from './player'
import * as subentity from './subentity'
import * as receiver from './receiver'

export abstract class client_event_handle {
    abstract on_kick_off(prompt_info:string):void;
    abstract on_transfer_complete():void;
}

export class app {
    public static instance:app;

    public ctx:context.context | null;
    public client_event_handle:client_event_handle | null;

    public player_mgr:player.player_manager;
    public subentity_mgr:subentity.subentity_manager;
    public receiver_mgr:receiver.receiver_manager;

    public on_conn: (() => void) | null;

    private __is_run__:boolean;
    private __conn_handle__:ConnMsgHandle.conn_msg_handle;
    private __entity_create_method__:Map<string, (id:string, info:object) => any>;
    private __hub_global_callback__:Map<string, (hub_name:string, data:Uint8Array) => void>;
    
    public constructor() {
        this.ctx = null;
        this.client_event_handle = null;

        this.on_conn = null;

        this.__is_run__ = true;
        this.__conn_handle__ = new ConnMsgHandle.conn_msg_handle();
        this.__entity_create_method__ = new Map<string, (id:string, info:object) => any>();
        this.__hub_global_callback__ = new Map<string, (hub_name:string, data:Uint8Array) => void>();

        this.player_mgr = new player.player_manager();
        this.subentity_mgr = new subentity.subentity_manager();
        this.receiver_mgr = new receiver.receiver_manager();

        app.instance = this;
    }

    public build(handle:client_event_handle) {
        this.client_event_handle = handle;
        setInterval(this.heartbeats.bind(this), 2000);
        return this;
    }

    private heartbeats() {
        if (this.ctx) {
            this.ctx.heartbeats();
        }
    }

    public connect_websocket(_ctx:context.context, wsHost:string) {
        this.ctx = _ctx;
        this.ctx.ConnectWebSocket(wsHost);
    }

    public connect_tcp(_ctx:context.context, host:string, port:number) {
        this.ctx = _ctx;
        this.ctx.ConnectTcp(host, port);
    }

    public on_kick_off(prompt_info:string) {
        if (this.client_event_handle) {
            this.client_event_handle.on_kick_off(prompt_info);
        }
    }

    public on_transfer_complete() {
        if (this.client_event_handle) {
            this.client_event_handle.on_transfer_complete();
        }
    }

    public on_call_global(method:string, hub_name:string, argvs:Uint8Array) {
        let _call_handle = this.__hub_global_callback__.get(method);
        if (_call_handle) {
            _call_handle.call(null, hub_name, argvs);
        }
    }

    public register_global_method(method:string, callback:(hub_name:string, data:Uint8Array) => void) {
        this.__hub_global_callback__.set(method, callback);
    }

    public login(sdk_uuid:string) : boolean {
        if (this.ctx) {
            return this.ctx.login(sdk_uuid)
        }
        return false;
    }

    public reconnect(account_id:string, token:string) : boolean {
        if (this.ctx) {
            return this.ctx.reconnect(account_id, token);
        }
        return false;
    }

    public request_hub_service(service_name:string) : boolean {
        if (this.ctx) {
            return this.ctx.request_hub_service(service_name);
        }
        return false;
    }

    public register(entity_type:string, creator:(id:string, info:object) => any) {
        this.__entity_create_method__.set(entity_type, creator);
    }

    public create_entity(entity_type:string, entity_id:string, argvs: object) {
        let _creator = this.__entity_create_method__.get(entity_type);
        if (_creator) {
            _creator.call(null, entity_id, argvs);
        }
    }

    public update_entity(entity_type:string, entity_id:string, argvs: object) {
        this.player_mgr.update_player(entity_id, argvs)
        this.subentity_mgr.update_subentity(entity_id, argvs)
        this.receiver_mgr.update_receiver(entity_id, argvs)
    }

    public delete_entity(entity_id:string) {
        this.player_mgr.del_player(entity_id);
        this.subentity_mgr.del_subentity(entity_id);
        this.receiver_mgr.del_receiver(entity_id);
    }

    public close() {
        this.__is_run__ = false;
    }

    public poll() {
        if (this.__is_run__) {
            if (this.ctx) {
                this.ctx.poll_conn_msg(this.__conn_handle__);
            }

            setTimeout(this.poll.bind(this), 16);
        }
    }

    public run() {
        this.poll();
    }
}