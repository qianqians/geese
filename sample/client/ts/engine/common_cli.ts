import * as engine from "./engine";
import { encode, decode } from "@msgpack/msgpack";
// this enum code is codegen by geese codegen for ts

export enum error_code {
    success = 0,
}

// this struct code is codegen by geese codegen for ts
export class role_rank_info {
     public role_name:string = ""
     public entity_id:string = ""
     public rank:number = 0
}

export function role_rank_info_to_protcol(_struct:role_rank_info) {
    let _protocol:any = {}
    _protocol["role_name"] = _struct.role_name
    _protocol["entity_id"] = _struct.entity_id
    _protocol["rank"] = _struct.rank
    return _protocol;
}

export function protcol_to_role_rank_info(_protocol:any) {
    let _struct = new role_rank_info()
    for (let key in _protocol) {
        let val = _protocol[key];
        if (key == "role_name") {
            _struct.role_name = val;
        }
        else if (key == "entity_id") {
            _struct.entity_id = val;
        }
        else if (key == "rank") {
            _struct.rank = val;
        }
    }
    return _struct;

}

export class client_time_info {
     public entity_id:string = ""
     public timetmp:number = 0
}

export function client_time_info_to_protcol(_struct:client_time_info) {
    let _protocol:any = {}
    _protocol["entity_id"] = _struct.entity_id
    _protocol["timetmp"] = _struct.timetmp
    return _protocol;
}

export function protcol_to_client_time_info(_protocol:any) {
    let _struct = new client_time_info()
    for (let key in _protocol) {
        let val = _protocol[key];
        if (key == "entity_id") {
            _struct.entity_id = val;
        }
        else if (key == "timetmp") {
            _struct.timetmp = val;
        }
    }
    return _struct;

}

// this module code is codegen by geese codegen for typescript


