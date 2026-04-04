import { _decorator, Component, Node } from 'cc';
const { ccclass, property } = _decorator;
import * as engine from './engine/engine' 

class ClientEventHandle extends engine.client_event_handle {
    public on_kick_off(prompt_info:string) {
        console.log(prompt_info);
    }

    public on_transfer_complete() {
        console.log("on_transfer_complete");
    }
}

class WSChannel extends engine.channel {
    private client: WebSocket | null;

    public constructor() {
        super();
        this.client = null;
    }

    public connect(wsHost:string) : boolean {
        console.log("WSChannel connect begin! wsHost:", wsHost);
        this.client = new WebSocket(wsHost);
        this.client.onopen = (evt) => {
            console.log("WSChannel connect complete! msg:", evt.type);
        }
        this.client.onclose = (evt) => {
            console.log("WSChannel onclose! msg:", evt.type);
        };
        this.client.onerror = (evt) => {
            console.log("WSChannel onerror! msg:", evt.type);
        };
        console.log("WSChannel connect end!");
        return true;
    }

    public send(data:Uint8Array) {
        if (this.client) {
            this.client.send(data);
        }
    }
    
    public on_recv(recv:(data:Uint8Array) => void) {
        if (this.client) {
            this.client.onmessage = (evt) =>{ 
                if (Buffer.isBuffer(evt.data)) {
                    recv(new Uint8Array(evt.data));
                }
                else if (Array.isArray(evt.data)) {
                    recv(new Uint8Array(Buffer.concat(evt.data)));
                }
                else if (evt.data instanceof ArrayBuffer) {
                    recv(new Uint8Array(evt.data));
                }
           };
        }
    }
}
   
class WSContext extends engine.context {
    public ConnectWebSocket(wsHost:string) : engine.channel {
        this.ch = new WSChannel();
        this.ch.connect(wsHost);
        this.ch.on_recv(this.recv.bind(this));
        return this.ch;
    }
}

@ccclass('new_driver')
export class new_driver extends Component {
    private _app: engine.app;

    start() {
        this._app = new engine.app();
        this._app.build(new ClientEventHandle());
        this._app.connect_websocket(new WSContext(), "ws://127.0.0.1:8100");
        this._app.on_conn = () => {
            engine.app.instance.login("1234567890qwerdsa", {})
        };
    }

    update(deltaTime: number) {
        this._app.poll();
    }
}