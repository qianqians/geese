/* tslint:disable */
/* eslint-disable */
/*
 * Autogenerated by @creditkarma/thrift-typescript v3.7.6
 * DO NOT EDIT UNLESS YOU ARE SURE THAT YOU KNOW WHAT YOU ARE DOING
*/
import * as thrift from "thrift";
export interface Iredis_msgArgs {
    server_name?: string;
    msg?: Buffer;
}
export class redis_msg {
    public server_name?: string;
    public msg?: Buffer;
    constructor(args?: Iredis_msgArgs) {
        if (args != null && args.server_name != null) {
            this.server_name = args.server_name;
        }
        if (args != null && args.msg != null) {
            this.msg = args.msg;
        }
    }
    public write(output: thrift.TProtocol): void {
        output.writeStructBegin("redis_msg");
        if (this.server_name != null) {
            output.writeFieldBegin("server_name", thrift.Thrift.Type.STRING, 1);
            output.writeString(this.server_name);
            output.writeFieldEnd();
        }
        if (this.msg != null) {
            output.writeFieldBegin("msg", thrift.Thrift.Type.STRING, 2);
            output.writeBinary(this.msg);
            output.writeFieldEnd();
        }
        output.writeFieldStop();
        output.writeStructEnd();
        return;
    }
    public static read(input: thrift.TProtocol): redis_msg {
        input.readStructBegin();
        let _args: any = {};
        while (true) {
            const ret: thrift.TField = input.readFieldBegin();
            const fieldType: thrift.Thrift.Type = ret.ftype;
            const fieldId: number = ret.fid;
            if (fieldType === thrift.Thrift.Type.STOP) {
                break;
            }
            switch (fieldId) {
                case 1:
                    if (fieldType === thrift.Thrift.Type.STRING) {
                        const value_1: string = input.readString();
                        _args.server_name = value_1;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                case 2:
                    if (fieldType === thrift.Thrift.Type.STRING) {
                        const value_2: Buffer = input.readBinary();
                        _args.msg = value_2;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                default: {
                    input.skip(fieldType);
                }
            }
            input.readFieldEnd();
        }
        input.readStructEnd();
        return new redis_msg(_args);
    }
}
