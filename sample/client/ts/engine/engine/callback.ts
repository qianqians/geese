/*
 * callback.ts
 * qianqians
 * 2023/10/5
 */

export class callback {
    public _callback : (Uint8Array) => void;
    public _error : (Uint8Array) => void;
    private _timeout : () => void;

    private release : () => void;

    public constructor(release_handle:()=>void) {
        this.release = release_handle;
    }

    public callback(rsp_callback:(Uint8Array) => void, err_callback:(Uint8Array) => void) {
        this._callback = rsp_callback;
        this._error = err_callback;
    }

    private __call_timeout__() {
        this.release();
        this._timeout();
    }

    public timeout(_timeout:number, time_callback:() => void) {
        this._timeout = time_callback;
        setTimeout(() => {
            this.__call_timeout__();
        }, _timeout);
    }
}