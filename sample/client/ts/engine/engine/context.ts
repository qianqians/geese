/*
 * context.ts
 * qianqians
 * 2023/10/5
 */
export abstract class channel {
    abstract connect(wsHost:string) : boolean;
    abstract send(data:Uint8Array) : void;
    abstract on_recv(recv:(data:Uint8Array) => void) : void;
}

import { TBufferedTransport, TCompactProtocol } from 'thrift'

import * as proto from './proto'
import * as ConnMsgHandle from './conn_msg_handle'
import * as app from './app'

export abstract class context {
    protected ch:channel|null = null;
    private conn_id:string|null = null;

    private offset:number = 0;
    private data:Uint8Array|null = null;
    private evs : proto.client_service[] = [];

    abstract ConnectWebSocket(wsHost:string) : channel;
    abstract ConnectTcp(host:string, port:number) : channel;

    public constructor() {
    }

    protected recv(data:Uint8Array) : void {
        console.log(`data:${data} recv begin!`);

        let u8data = new Uint8Array(data);
            
        let new_data:Uint8Array|null = new Uint8Array(this.offset + u8data.byteLength);
        if (this.data !== null){
            new_data.set(this.data);
        }
        new_data.set(u8data, this.offset);

        while(new_data && new_data.length > 4) {
            let len = new_data[0] | new_data[1] << 8 | new_data[2] << 16 | new_data[3] << 24;

            if ( (len + 4) > new_data.length ){
                break;
            }
            console.log(`data.length:${len}, new_data.length:${new_data.length}`);

            var str_bytes = new_data.subarray(4, (len + 4));
            let recvFun = TBufferedTransport.receiver((trans,  seqid) => {
                let input = new TCompactProtocol(trans);
                let ev = proto.client_service.read(input);
                this.evs.push(ev);
            }, 0);
            recvFun(Buffer.from(str_bytes));

            if ( new_data.length > (len + 4) ){
                let _data:Uint8Array = new Uint8Array(new_data.length - (len + 4));
                _data.set(new_data.subarray(len + 4));
                new_data = _data;
            }
            else{
                new_data = null;
                break;
            }
        }

        this.data = new_data;
        if (new_data !== null){
            this.offset = new_data.length;
        }else{
            this.offset = 0;
        }
    }

    public send(service:proto.gate_client_service) {
        let trans = new TBufferedTransport(undefined, (msg) => {
            if (msg) {
                console.log("send begin!");
                var data = Uint8Array.from(msg);

                var send_data = new Uint8Array(4 + data.length);
                send_data[0] = data.length & 0xff;
                send_data[1] = (data.length >> 8) & 0xff;
                send_data[2] = (data.length >> 16) & 0xff;
                send_data[3] = (data.length >> 24) & 0xff;
                send_data.set(data, 4);

                if (this.ch) {
                    console.log("ch send begin!");
                    this.ch.send(send_data); 
                    console.log("ch send end!");
                }
            }
        });
        let output = new TCompactProtocol(trans);
        service.write(output);
        trans.flush();
    }

    public login(sdk_uuid:string) : boolean {
        let loginData = new proto.client_request_hub_login();
        loginData.sdk_uuid = sdk_uuid;
        let reqData = proto.gate_client_service.fromLogin(loginData);
        
        this.send(reqData);

        return true;
    }

    public reconnect(account_id:string, token:string) : boolean {
        let recData = new proto.client_request_hub_reconnect();
        recData.account_id = account_id;
        recData.token = token;
        let reqData = proto.gate_client_service.fromReconnect(recData);

        this.send(reqData);

        return true;
    }

    public request_hub_service(service_name:string) : boolean {
        let svcData = new proto.client_request_hub_service();
        svcData.service_name = service_name;
        let reqData = proto.gate_client_service.fromRequest_hub_service(svcData);

        this.send(reqData);

        return true;
    }

    public call_rpc(entity_id:string, msg_cb_id:number, method:string, argvs:Uint8Array) : boolean {
        let msg = new proto.msg();
        msg.method = method;
        msg.argvs = Buffer.from(argvs);
        let rpcData = new proto.client_call_hub_rpc({
            entity_id: entity_id,
            msg_cb_id: msg_cb_id,
            message: msg
        });
        let reqData = proto.gate_client_service.fromCall_rpc(rpcData);
        
        this.send(reqData);

        return true;
    }

    public call_rsp(entity_id:string, msg_cb_id:number, argvs:Uint8Array) : boolean {
        let rspArgsData = new proto.rpc_rsp({
            entity_id: entity_id,
            msg_cb_id: msg_cb_id,
            argvs: Buffer.from(argvs)
        });
        let rspData = new proto.client_call_hub_rsp({
            rsp: rspArgsData
        });
        let reqData = proto.gate_client_service.fromCall_rsp(rspData);
        
        this.send(reqData);

        return true;
    }

    public call_err(entity_id:string, msg_cb_id:number, argvs:Uint8Array) : boolean {
        let errArgsData = new proto.rpc_err({
            entity_id: entity_id,
            msg_cb_id: msg_cb_id,
            argvs: Buffer.from(argvs)
        });
        let errData = new proto.client_call_hub_err({
            err: errArgsData
        });
        let reqData = proto.gate_client_service.fromCall_err(errData);
        
        this.send(reqData);

        return true;
    }

    public call_ntf(entity_id:string, method:string, argvs:Uint8Array) : boolean {
        let msg = new proto.msg();
        msg.method = method;
        msg.argvs = Buffer.from(argvs);
        let ntfData = new proto.client_call_hub_ntf({
            entity_id: entity_id,
            message: msg
        });
        let reqData = proto.gate_client_service.fromCall_ntf(ntfData);
        
        this.send(reqData);

        return true;
    }

    public heartbeats() {
        console.log("call heartbeats!")
        let data = new proto.client_call_gate_heartbeats();
        let reqData = proto.gate_client_service.fromHeartbeats(data);
        this.send(reqData);
    }

    public poll_conn_msg(handle:ConnMsgHandle.conn_msg_handle) : boolean {
        let ev = this.evs.pop();
        if (!ev) {
            return false;
        }
        console.log(`poll_conn_msg ev begin!`);

        if (ev.conn_id) {
            console.log(`poll_conn_msg ev ev.conn_id begin!`);
            if (ev.conn_id.conn_id) {
                this.conn_id = ev.conn_id.conn_id;
                if (app.app.instance.on_conn) {
                    app.app.instance.on_conn.call(null);
                }
            }
        }
        else if (ev.heartbeats) {
            this.heartbeats();
        }
        else if (ev.create_remote_entity) {
            console.log(`poll_conn_msg ev ev.create_remote_entity begin!`);
            let event = ev.create_remote_entity;
            if (event.entity_type && event.entity_id && event.argvs) {
                console.log(`poll_conn_msg ev ev.create_remote_entity event begin!`);
                handle.on_create_remote_entity(event.entity_type, event.entity_id, Uint8Array.from(event.argvs));
            }
        }
        else if (ev.delete_remote_entity) {
            let event = ev.delete_remote_entity;
            if (event.entity_id) {
                handle.on_delete_remote_entity(event.entity_id);
            }
        }
        else if (ev.kick_off) {
            let event = ev.kick_off;
            if (event.prompt_info) {
                handle.on_kick_off(event.prompt_info);
            }
        }
        else if (ev.transfer_complete) {
            handle.on_transfer_complete();
        }
        else if (ev.call_rpc) {
            let event = ev.call_rpc;
            if (event.hub_name && event.entity_id && event.msg_cb_id && event.message && event.message.method && event.message.argvs) {
                handle.on_call_rpc(event.hub_name, event.entity_id, event.msg_cb_id.toNumber(), event.message.method, event.message.argvs);
            }
        }
        else if (ev.call_rsp) {
            let event = ev.call_rsp;
            if (event.rsp && event.rsp.entity_id && event.rsp.msg_cb_id && event.rsp.argvs) {
                handle.on_call_rsp(event.rsp.entity_id, event.rsp.msg_cb_id.toNumber(), event.rsp.argvs);
            }
        }
        else if (ev.call_err) {
            let event = ev.call_err;
            if (event.err && event.err.entity_id && event.err.msg_cb_id && event.err.argvs) {
                handle.on_call_err(event.err.entity_id, event.err.msg_cb_id.toNumber(), event.err.argvs);
            }
        }
        else if (ev.call_ntf) {
            let event = ev.call_ntf;
            if (event.hub_name && event.entity_id && event.message && event.message.method && event.message.argvs) {
                handle.on_call_ntf(event.hub_name, event.entity_id, event.message.method, event.message.argvs);
            }
        }
        else if (ev.call_global) {
            let event = ev.call_global;
            if (event.hub_name && event.message && event.message.method && event.message.argvs) {
                handle.on_call_global(event.message.method, event.hub_name, event.message.argvs);
            }
        }

        return true;
    }
}