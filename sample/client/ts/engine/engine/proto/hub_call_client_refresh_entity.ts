/* tslint:disable */
/* eslint-disable */
/*
 * Autogenerated by @creditkarma/thrift-typescript v3.7.6
 * DO NOT EDIT UNLESS YOU ARE SURE THAT YOU KNOW WHAT YOU ARE DOING
*/
import * as thrift from "thrift";
export interface Ihub_call_client_refresh_entityArgs {
    conn_id?: string;
    is_main?: boolean;
    entity_id?: string;
    entity_type?: string;
    argvs?: Buffer;
}
export class hub_call_client_refresh_entity {
    public conn_id?: string;
    public is_main?: boolean;
    public entity_id?: string;
    public entity_type?: string;
    public argvs?: Buffer;
    constructor(args?: Ihub_call_client_refresh_entityArgs) {
        if (args != null && args.conn_id != null) {
            this.conn_id = args.conn_id;
        }
        if (args != null && args.is_main != null) {
            this.is_main = args.is_main;
        }
        if (args != null && args.entity_id != null) {
            this.entity_id = args.entity_id;
        }
        if (args != null && args.entity_type != null) {
            this.entity_type = args.entity_type;
        }
        if (args != null && args.argvs != null) {
            this.argvs = args.argvs;
        }
    }
    public write(output: thrift.TProtocol): void {
        output.writeStructBegin("hub_call_client_refresh_entity");
        if (this.conn_id != null) {
            output.writeFieldBegin("conn_id", thrift.Thrift.Type.STRING, 1);
            output.writeString(this.conn_id);
            output.writeFieldEnd();
        }
        if (this.is_main != null) {
            output.writeFieldBegin("is_main", thrift.Thrift.Type.BOOL, 2);
            output.writeBool(this.is_main);
            output.writeFieldEnd();
        }
        if (this.entity_id != null) {
            output.writeFieldBegin("entity_id", thrift.Thrift.Type.STRING, 3);
            output.writeString(this.entity_id);
            output.writeFieldEnd();
        }
        if (this.entity_type != null) {
            output.writeFieldBegin("entity_type", thrift.Thrift.Type.STRING, 4);
            output.writeString(this.entity_type);
            output.writeFieldEnd();
        }
        if (this.argvs != null) {
            output.writeFieldBegin("argvs", thrift.Thrift.Type.STRING, 5);
            output.writeBinary(this.argvs);
            output.writeFieldEnd();
        }
        output.writeFieldStop();
        output.writeStructEnd();
        return;
    }
    public static read(input: thrift.TProtocol): hub_call_client_refresh_entity {
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
                    if (fieldType === thrift.Thrift.Type.BOOL) {
                        const value_2: boolean = input.readBool();
                        _args.is_main = value_2;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                case 3:
                    if (fieldType === thrift.Thrift.Type.STRING) {
                        const value_3: string = input.readString();
                        _args.entity_id = value_3;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                case 4:
                    if (fieldType === thrift.Thrift.Type.STRING) {
                        const value_4: string = input.readString();
                        _args.entity_type = value_4;
                    }
                    else {
                        input.skip(fieldType);
                    }
                    break;
                case 5:
                    if (fieldType === thrift.Thrift.Type.STRING) {
                        const value_5: Buffer = input.readBinary();
                        _args.argvs = value_5;
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
        return new hub_call_client_refresh_entity(_args);
    }
}
