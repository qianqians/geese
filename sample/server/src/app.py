import sys
from engine.engine import *
from engine.login_svr import *
from engine.update_rank_svr import *

rankImpl = None

class RankSubEntity(subentity):
    def __init__(self, source_hub_name: str, entity_type: str, entity_id: str) -> None:
        super().__init__(source_hub_name, entity_type, entity_id)
        self.update_rank_caller = update_rank_caller(self)

    def call_update_rank(self, playerEntityId):
        self.update_rank_caller.call_update_rank(playerEntityId).callBack(
            lambda: self.trace("call_update_rank cb"),
            lambda err: self.trace("call_update_rank err:{}", err))

    def Creator(source_hub_name:str, entity_id:str, description: dict):
        app().trace(f"RankSubEntity Creator source_hub_name:{source_hub_name} entity_id:{entity_id}")
        global rankImpl
        rankImpl = RankSubEntity(source_hub_name, "RankImpl", entity_id)
        return rankImpl

class SamplePlayer(player):
    def __init__(self, entity_id: str, gate_name: str, conn_id: str, accound_id: int):
        player.__init__(self, "login", "SamplePlayer", entity_id, gate_name, conn_id, False)
        self.login_module = login_module(self)
        self.login_module.on_login.append(lambda rsp, _sdk_uuid: self.login_callback(rsp, _sdk_uuid))
        self.accound_id = accound_id
        
    def full_info(self) -> dict:
        return {"accound_id":self.accound_id}

    def hub_info(self) -> dict:
        return {}
    
    def client_info(self) -> dict:
        return {}
    
    def on_migrate_to_other_hub(self, migrate_hub:str):
        pass
    
    def login_callback(self, rsp:login_login_rsp, _sdk_uuid:str):
        app().trace(f"SamplePlayer login_callback _sdk_uuid:{_sdk_uuid}")
        rsp.rsp(True)

class LoginEventHandle(login_event_handle):
    def __init__(self, db:str, collection:str):
        super().__init__(db, collection)
        
        self.__get_guid_handle__ = get_guid("sample", "account_uuid")

    async def __get_client_account_id__(self, sdk_uuid:str):
        app().trace("LoginEventHandle __get_client_account_id__!")
        uuidObj = await self.__get_dbproxy__().get_object_one(self.__db__, self.__collection__, {"SDK_UUID":sdk_uuid})
        if not uuidObj:
            return await self.__get_guid_handle__.gen()
        else:
            return uuidObj["GUID"]
        
    async def on_login(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, argvs:dict):
        app().trace("LoginEventHandle on_login!")
        accound_id = await self.__get_client_account_id__(sdk_uuid)
        info_str = app().redis_proxy.get("sample:player_info:{}".format(accound_id))
        app().trace("LoginEventHandle on_login! redis_cache info_str:{}".format(info_str))
        if info_str is not None and info_str != "":
            info = json.loads(info_str)
            app().trace("LoginEventHandle on_login! info:{}".format(info))
            self.__replace_client__(info["gate"], info["conn_id"], new_gate_name, new_conn_id, False, "其他位置登录!")
        else:
            _p = SamplePlayer(str(uuid.uuid4()), new_gate_name, new_conn_id, accound_id)
            app().player_mgr.add_player(_p)
            app().trace("LoginEventHandle on_login! create_main_remote_entity:{} begin!".format(_p))
            _p.create_main_remote_entity()
            app().trace("LoginEventHandle on_login! create_main_remote_entity:{}".format(_p))
            app().trace("LoginEventHandle on_login! call_update_rank rankImpl:{}".format(rankImpl))
            rankImpl.call_update_rank(_p.entity_id)
            app().trace("LoginEventHandle on_login end!")
        app().redis_proxy.set("sample:player_info:{}".format(accound_id), json.dumps({"gate":new_gate_name, "conn_id":new_conn_id}))
    
    async def on_reconnect(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str):
        app().trace("LoginEventHandle on_reconnect!")
        accound_id = await self.__get_client_account_id__(sdk_uuid)
        info_str = app().redis_proxy.get("sample:player_info:{}".format(accound_id))
        if info_str is not None and info_str != "":
            info = json.loads(info_str)
            app().trace("LoginEventHandle on_reconnect! info:{}".format(info))
            self.__replace_client__(info["gate"], info["conn_id"], new_gate_name, new_conn_id, True, "其他位置登录!")
        else:
            _p = SamplePlayer(str(uuid.uuid4()), new_gate_name, new_conn_id, accound_id)
            app().player_mgr.add_player(_p)
            app().trace("LoginEventHandle on_reconnect! create_main_remote_entity:{} begin!".format(_p))
            _p.create_main_remote_entity()
            app().trace("LoginEventHandle on_reconnect! create_main_remote_entity:{}".format(_p))
            app().trace("LoginEventHandle on_reconnect! call_update_rank rankImpl:{}".format(rankImpl))
            rankImpl.call_update_rank(_p.entity_id)
            app().trace("LoginEventHandle on_reconnect end!")
        app().redis_proxy.set("sample:player_info:{}".format(accound_id), json.dumps({"gate":new_gate_name, "conn_id":new_conn_id}))
    
class PlayerEventHandle(player_event_handle):
    def player_offline(self, _player:player) -> dict:
        info = _player.full_info()
        app().redis_proxy.delete("sample:player_info:{}".format(info["accound_id"]))
        return info
    
def main(cfg_file:str):
    _app = app()
    _app.build(cfg_file)
    _app.build_login_service(LoginEventHandle("sample", "account"))
    _app.build_player_service(PlayerEventHandle())
    _app.register_service("login")
    _app.register("RankImpl", RankSubEntity.Creator)
    _app.run_coroutine_async(query_service("Rank"))
    _app.run()
    
if __name__ == '__main__':
    main(sys.argv[1])