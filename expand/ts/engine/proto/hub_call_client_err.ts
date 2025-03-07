/* tslint:disable */
/* eslint-disable */
/*
 * Autogenerated by @creditkarma/thrift-typescript v3.7.6
 * DO NOT EDIT UNLESS YOU ARE SURE THAT YOU KNOW WHAT YOU ARE DOING
*/
import * as thrift from "thrift";
import * as __ROOT_NAMESPACE__ from "./";
export interface Ihub_call_client_errArgs {
    conn_id?: string;
    err?: __ROOT_NAMESPACE__.rpc_err;
}
export class hub_call_client_err {
    public conn_id?: string;
    public err?: __ROOT_NAMESPACE__.rpc_err;
    constructor(args?: Ihub_call_client_errArgs) {
        if (args != null && args.conn_id != null) {
            this.conn_id = args.conn_id;
        }
        if (args != null && args.err != null) {
            this.err = args.err;
        }
    }
    public write(output: thrift.TProtocol): void {
        output.writeStructBegin("hub_call_client_err");
        if (this.conn_id != null) {
            output.writeFieldBegin("conn_id", thrift.Thrift.Type.STRING, 1);
            output.writeString(this.conn_id);
            output.writeFieldEnd();
        }
        if (this.err != null) {
            output.writeFieldBegin("err", thrift.Thrift.Type.STRUCT, 2);
            this.err.write(output);
            output.writeFieldEnd();
        }
        output.writeFieldStop();
        output.writeStructEnd();
        return;
    }
    public static read(input: thrift.TProtocol): hub_call_client_err {
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
                        _args.conn_id = value_1;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                case 2:
                    if (fieldType === thrift.Thrift.Type.STRUCT) {
                        const value_2: __ROOT_NAMESPACE__.rpc_err = __ROOT_NAMESPACE__.rpc_err.read(input);
                        _args.err = value_2;
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
        return new hub_call_client_err(_args);
    }
}
