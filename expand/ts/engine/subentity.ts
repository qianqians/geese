/*
 * subentity.ts
 * qianqians
 * 2023/10/5
 */
import * as Base from './base_entity'
import * as CallBack from './callback'
import * as app1 from './app'

export abstract class subentity extends Base.base_entity {
    public request_msg_cb_id:number;
    public hub_notify_callback:Map<string, (source:string, data:Uint8Array) => void>;
    public hub_callback:Map<number, CallBack.callback>;

    public constructor(entity_type:string, entity_id:string) {
        super(entity_type, entity_id);

        this.request_msg_cb_id = Math.floor(Math.random() * 10011);
        this.hub_notify_callback = new Map<string, (source:string, data:Uint8Array) => void>();
        this.hub_callback = new Map<number, CallBack.callback>();

        app1.app.instance.subentity_mgr.add_subentity(this);
    }

    abstract update_subentity(argvs: object): object; 

    public del_callback(msg_cb_id:number) : boolean {
        return this.hub_callback.delete(msg_cb_id);
    }
    
    public handle_hub_response(msg_cb_id:number, argvs:Uint8Array) {
        let _call_handle = this.hub_callback.get(msg_cb_id);
        if (_call_handle) {
            _call_handle._callback(argvs);
            this.hub_callback.delete(msg_cb_id);
        }
    }
    
    public handle_hub_response_error(msg_cb_id:number, argvs:Uint8Array) {
        let _call_handle = this.hub_callback.get(msg_cb_id);
        if (_call_handle) {
            _call_handle._error(argvs);
            this.hub_callback.delete(msg_cb_id);
        }
    }
    
    public handle_hub_notify(method:string, hub_name:string, argvs:Uint8Array) {
        let _call_handle = this.hub_notify_callback.get(method);
        if (_call_handle) {
            _call_handle.call(null, hub_name, argvs);
        }
    }
    
    public reg_hub_notify_callback(method:string, callback:(source:string, argvs:Uint8Array) => void) {
        this.hub_notify_callback.set(method, callback);
    }
    
    public call_hub_request(method:string, argvs:Uint8Array) : number {
        let msg_cb_id = this.request_msg_cb_id
        this.request_msg_cb_id += 1
        app1.app.instance.ctx.call_rpc(this.EntityID, msg_cb_id, method, argvs);
        return msg_cb_id
    }
    
    public reg_hub_callback(msg_cb_id:number, rsp:CallBack.callback) {
        this.hub_callback.set(msg_cb_id, rsp);
    }
    
    public call_hub_notify(method:string, argvs:Uint8Array) {
        app1.app.instance.ctx.call_ntf(this.EntityID, method, argvs);
    }
}

export class subentity_manager {
    private subentities:Map<string, subentity>;

    public constructor() {
        this.subentities = new Map<string, subentity>();
    }

    public add_subentity(_entity:subentity) {
        this.subentities.set(_entity.EntityID, _entity);
    }

    public update_subentity(entity_id:string, argvs: object) {
        let _subentity = this.get_subentity(entity_id);
        _subentity?.update_subentity(argvs);
    }

    public get_subentity(entity_id:string) : subentity | undefined{
        return this.subentities.get(entity_id);
    }

    public del_subentity(entity_id:string) {
        this.subentities.delete(entity_id);
    }
}