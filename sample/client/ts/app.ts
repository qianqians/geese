import * as engine from './engine/engine' 
import * as login_cli from './engine/login_cli'
import * as get_rank_cli from './engine/get_rank_cli'

class ClientEventHandle extends engine.client_event_handle {
    public on_kick_off(prompt_info:string) {
        console.log(prompt_info);
    }

    public on_transfer_complete() {
        console.log("on_transfer_complete");
    }
}

let playerImpl:SamplePlayer|null = null

class RankSubEntity extends engine.subentity {
    private _get_rank_caller: get_rank_cli.get_rank_caller;

    public constructor(entity_type: string, entity_id: string) {
        super(entity_type, entity_id)
        this._get_rank_caller = new get_rank_cli.get_rank_caller(this);
    }

    public get_self_rank(entity_id) {
        this._get_rank_caller.get_self_rank(entity_id).callBack(
            (_info) => { console.log(`RankSubEntity get_self_rank callBack:${_info}`) },
            (_err) => { console.log(`RankSubEntity get_self_rank err:{_err}`) }).timeout(
                1000, () => { console.log(`RankSubEntity get_self_rank timeout!`)});
    }
        

    public update_subentity(argvs: object) {
        console.log(`RankSubEntity:{self.entity_id} update_subentity!`);
        return argvs;
    }

    public static Creator(entity_id:string, description: object) {
        console.log(`RankSubEntity Creator entity_id:{entity_id}`);
        let rankImpl = new RankSubEntity("RankImpl", entity_id);
        rankImpl.get_self_rank(playerImpl?.EntityID)
        return rankImpl
    }
}

class SamplePlayer extends engine.player {
    private _login_caller: login_cli.login_caller;

    public constructor(entity_id: string) {
        super("SamplePlayer", entity_id)
        this._login_caller = new login_cli.login_caller(this);
    }
    
    public update_player(argvs: object) {
        console.log(`SamplePlayer:{self.entity_id} update_player!`);
    }

    public static Creator(entity_id: string, description: object) {
        console.log(`SamplePlayer:{entity_id}`);
        playerImpl = new SamplePlayer(entity_id)
        playerImpl._login_caller.login("entity_id-123456").callBack(
            (success) => { console.log(`SamplePlayer login success:{success}`) },
            (_err) => { console.log(`SamplePlayer login _err:{_err}`) } ).timeout(
            1000, () => { console.log(`SamplePlayer login timeout!`) } );
            engine.app.instance.request_hub_service("Rank")
        return playerImpl
    }
}
    

function conn_callback(conn_id:string) {
    console.log("conn_callback begin!");
    engine.app.instance.login(engine.uuid.v4())
    console.log("conn_callback end!")
}

function main() {
    let _app = new engine.app()
    _app.build(new ClientEventHandle())
    _app.register("SamplePlayer", SamplePlayer.Creator)
    _app.register("RankImpl", RankSubEntity.Creator)
    //_app.connect_tcp("127.0.0.1", 8000, conn_callback)
    console.log("run begin!")
    _app.run()
}
main();