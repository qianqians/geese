/*
 * receiver.ts
 * qianqians
 * 2023/10/5
 */
import * as Base from './base_entity'

export abstract class receiver extends Base.base_entity {
    private hub_notify_callback:Map<string, (source:string, data:Uint8Array) => void>;
    
    public constructor(entity_type:string, entity_id:string) {
        super(entity_type, entity_id);

        this.hub_notify_callback = new Map<string, (source:string, data:Uint8Array) => void>();
    }

    abstract update_receiver(argvs: object): object; 

    public handle_hub_notify(method:string, hub_name:string, argvs:Uint8Array) {
        let _callback = this.hub_notify_callback.get(method);
        if (_callback) {
            _callback.call(null, hub_name, argvs);
        }
    }

    public reg_hub_notify_callback(method:string, callback:(source:string, data:Uint8Array) => void) {
        this.hub_notify_callback.set(method, callback);
    }
}

export class receiver_manager {
    private receivers:Map<string, receiver>;

    public constructor() {
        this.receivers = new Map<string, receiver>();
    }

    public add_receiver(_receiver:receiver) {
        this.receivers.set(_receiver.EntityID, _receiver);
    }

    public update_receiver(entity_id:string, argvs: object) {
        let _receiver = this.get_receiver(entity_id);
        _receiver?.update_receiver(argvs);
    }

    public get_receiver(entity_id:string) : receiver | undefined {
        return this.receivers.get(entity_id);
    }

    public del_receiver(entity_id:string) {
        this.receivers.delete(entity_id);
    }
}