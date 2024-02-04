import * as engine from "./engine";
import { encode, decode } from "@msgpack/msgpack";
import * as common from "./common_cli";
// this enum code is codegen by geese codegen for ts

// this struct code is codegen by geese codegen for ts
// this caller code is codegen by geese codegen for typescript
export class login_login_cb {
    public entity:engine.subentity|engine.player;
    public cb:((is_displace:boolean) => void)|null = null;
    public err:((err:common.error_code) => void)|null = null;
    public rsp:engine.callback;
    public constructor(_cb_uuid:number, _entity:engine.subentity|engine.player) {
        this.entity = _entity
        this.rsp = new engine.callback(() => { return this.entity.del_callback(_cb_uuid); });
        this.entity.reg_hub_callback(_cb_uuid, this.rsp)

    }

    private on_rsp(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _is_displace = inArray[0];
        if (this.cb) this.cb(_is_displace);

    }

    private on_err(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _err = inArray[0];
        if (this.err) this.err(_err)

    }

    public callBack(_cb:(is_displace:boolean) => void, _err:(err:common.error_code) => void) {
        this.cb = _cb;
        this.err = _err;
        this.rsp.callback(this.on_rsp, this.on_err);
        return this.rsp;
    }

}

export class login_caller {
    public entity:engine.subentity|engine.player;
    public constructor(entity:engine.subentity|engine.player) {
        this.entity = entity;
    }

    public  login(sdk_uuid:string) {
        let _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1:any[] = []
        _argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1.push(sdk_uuid);
        let _cb_uuid = this.entity.call_hub_request("login", encode(_argv_d3bb20a7_d0fc_3440_bb9e_b3cc0630e2d1));
        return new login_login_cb(_cb_uuid, this.entity);
    }

}


