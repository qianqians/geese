/*
 * callback.ts
 * qianqians
 * 2023/10/5
 */

export class callback {
    public _callback : null | ((arg:Uint8Array) => void);
    public _error : null | ((arg:Uint8Array) => void);
    private _timeout : null | (() => void);

    private release : () => boolean;

    public constructor(release_handle:()=>boolean) {
        this._callback = null;
        this._error = null;
        this._timeout = null;

        this.release = release_handle;
    }

    public callback(rsp_callback:((arg:Uint8Array) => void), err_callback:((arg:Uint8Array) => void)) {
        this._callback = rsp_callback;
        this._error = err_callback;
    }

    private __call_timeout__() {
        if (this.release() && this._timeout) {
            this._timeout();
        }
    }

    public timeout(_timeout:number, time_callback:() => void) {
        this._timeout = time_callback;
        setTimeout(() => {
            this.__call_timeout__();
        }, _timeout);
    }
}