import * as engine from "./engine";
import { encode, decode } from "./@msgpack/msgpack";
import * as common from "./common_cli";
// this enum code is codegen by geese codegen for ts

// this struct code is codegen by geese codegen for ts
// this caller code is codegen by geese codegen for typescript
export class get_rank_get_self_rank_cb {
    public entity:engine.subentity;
    public cb:(rank:common.role_rank_info) => void;
    public err:(err:common.error_code) => void;
    public rsp:engine.callback;
    public constructor(_cb_uuid:number, _entity:engine.subentity|engine.player) {
        this.entity = _entity
        this.rsp = new engine.callback(() => { this.entity.del_callback(_cb_uuid); });
        this.entity.reg_hub_callback(_cb_uuid, this.rsp)

    }

    private on_rsp(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _rank = common.protcol_to_role_rank_info(inArray[0]);
        this.cb(_rank);

    }

    private on_err(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _err = inArray[0];
        this.err(_err)

    }

    public  callBack(_cb:(rank:common.role_rank_info) => void, _err:(err:common.error_code) => void) {
        this.cb = _cb;
        this.err = _err;
        this.rsp.callback(this.on_rsp, this.on_err);
        return this.rsp;
    }

}

export class get_rank_get_rank_cb {
    public entity:engine.subentity;
    public cb:(rank_list:Array<common.role_rank_info>) => void;
    public err:(err:common.error_code) => void;
    public rsp:engine.callback;
    public constructor(_cb_uuid:number, _entity:engine.subentity|engine.player) {
        this.entity = _entity
        this.rsp = new engine.callback(() => { this.entity.del_callback(_cb_uuid); });
        this.entity.reg_hub_callback(_cb_uuid, this.rsp)

    }

    private on_rsp(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _rank_list:Array<common.role_rank_info> = [];
        for (let v_e249ecbd_d64c_526b_901b_6d4ddee4b75a of inArray[0]) {
            _rank_list.push(common.protcol_to_role_rank_info(v_e249ecbd_d64c_526b_901b_6d4ddee4b75a))
        }
        this.cb(_rank_list);

    }

    private on_err(bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _err = inArray[0];
        this.err(_err)

    }

    public  callBack(_cb:(rank_list:Array<common.role_rank_info>) => void, _err:(err:common.error_code) => void) {
        this.cb = _cb;
        this.err = _err;
        this.rsp.callback(this.on_rsp, this.on_err);
        return this.rsp;
    }

}

export class get_rank_caller {
    public entity:engine.subentity|engine.player;
    public constructor(entity:engine.subentity|engine.player) {
        this.entity = entity;
    }

    public  get_self_rank(entity_id:string) {
        let _argv_e22ae90d_2428_3197_a8fb_549203f714e0:any[] = []
        _argv_e22ae90d_2428_3197_a8fb_549203f714e0.push(entity_id);
        let _cb_uuid = this.entity.call_hub_request("get_self_rank", encode(_argv_e22ae90d_2428_3197_a8fb_549203f714e0));
        return new get_rank_get_self_rank_cb(_cb_uuid, this.entity);
    }

    public  get_rank(start:number, end:number) {
        let _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7:any[] = []
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.push(start);
        _argv_e869f1c8_1f14_384f_aba6_2af2b54335e7.push(end);
        let _cb_uuid = this.entity.call_hub_request("get_rank", encode(_argv_e869f1c8_1f14_384f_aba6_2af2b54335e7));
        return new get_rank_get_rank_cb(_cb_uuid, this.entity);
    }

}


