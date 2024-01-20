import * as engine from "./engine";
import { encode, decode } from "./@msgpack/msgpack";
import * as common from "./common_cli";
// this enum code is codegen by geese codegen for ts

// this struct code is codegen by geese codegen for ts
// this module code is codegen by geese codegen for typescript
export class notice_module {
    public on_notice:((session, string) => void)[] = []
    public constructor() {
        engine.app.instance.register_global_method("notice", this.notice)
    }

    public notice(hub_name:string, bin:Uint8Array) {
        let inArray = decode(bin) as any;
        let _msg = inArray[0];
        let s = new engine.session(hub_name)
        for (let fn of this.on_notice) {
            fn(s, _msg);
        }
    }

}


