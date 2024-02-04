import * as engine from "./engine";
import { encode, decode } from "@msgpack/msgpack";
import * as common from "./common_cli";
// this enum code is codegen by geese codegen for ts

// this struct code is codegen by geese codegen for ts
// this module code is codegen by geese codegen for typescript
export class heartbeat_call_heartbeat_rsp extends engine.session {
    public entity:engine.player;
    public is_rsp:boolean = false;
    public msg_cb_id:number;
    public constructor(hub_name:string, msg_cb_id:number, entity:engine.player) {
        super(hub_name);
        this.entity = entity;
        this.msg_cb_id = msg_cb_id;
    }

    public rsp(time_info:common.client_time_info) {
        if (this.is_rsp) {
            return
        }
        this.is_rsp = false;

        let _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f:any[] = [];
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f.push(common.client_time_info_to_protcol(time_info));
        this.entity.call_hub_response(this.msg_cb_id, encode(_argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f));
    }

    public err(err:common.error_code) {
        if (this.is_rsp) {
            return
        }
        this.is_rsp = false;

        let _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f:any[] = [];
        _argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f.push(err);
        this.entity.call_hub_response_error(this.msg_cb_id, encode(_argv_8e2b295f_c4a8_3b9d_91eb_91a5bec35b0f));
    }

}

export class heartbeat_module {
    public entity:engine.player;
    public on_call_heartbeat:((heartbeat_call_heartbeat_rsp, string) => void)[] = []
    public constructor(entity:engine.player) {
        this.entity = entity

        this.entity.reg_hub_request_callback("call_heartbeat", this.call_heartbeat)
    }

    public call_heartbeat(hub_name:string, msg_cb_id:number, bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _entity_id = inArray[1];
        let rsp = new heartbeat_call_heartbeat_rsp(hub_name, msg_cb_id, this.entity)
        for (let fn of this.on_call_heartbeat) {
            fn(rsp, _entity_id)
;
        }
    }

}


