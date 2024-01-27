import sys
from engine.engine import *
from engine.login_svr import *
from engine.update_rank_svr import *

class RankSubEntity(subentity):
    def __init__(self, source_hub_name: str, entity_type: str, entity_id: str) -> None:
        super().__init__(source_hub_name, entity_type, entity_id)
        self.update_rank_caller = update_rank_caller(self)

    def call_update_rank(self, playerEntityId):
        self.update_rank_caller.call_update_rank(playerEntityId)

    def Creator(source_hub_name:str, entity_id:str, description: dict):
        print(f"RankSubEntity Creator source_hub_name:{source_hub_name} entity_id:{entity_id}")
        rankImpl = RankSubEntity(source_hub_name, entity_id, description)
        return rankImpl

rankImpl:RankSubEntity = None

class SamplePlayer(player):
    def __init__(self, entity_id: str, gate_name: str, conn_id: str):
        player.__init__(self, "SamplePlayer", entity_id, gate_name, conn_id)
        self.login_module = login_module(self)
        
        self.login_module.on_login.append(lambda rsp, _sdk_uuid: self.login_callback(rsp, _sdk_uuid))
        
    def hub_info(self) -> dict:
        return {}
    
    def client_info(self) -> dict:
        return {}
    
    def login_callback(self, rsp:login_login_rsp, _sdk_uuid:str):
        print(f"SamplePlayer login_callback _sdk_uuid:{_sdk_uuid}")
        rsp.rsp(True)

class LoginEventHandle(login_event_handle):
    def __init__(self, db:str, collection:str):
        super().__init__(db, collection)
        
        self.__get_guid_handle__ = get_guid("sample", "account_uuid")

    async def __get_client_account_id__(self, sdk_uuid:str):
        print("LoginEventHandle __get_client_account_id__!")
        uuidObj = await self.__get_dbproxy__().get_object_one(self.__db__, self.__collection__, {"SDK_UUID":sdk_uuid})
        if not uuidObj:
            return await self.__get_guid_handle__.gen()
        else:
            return uuidObj["GUID"]
        
    async def on_login(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str):
        print("LoginEventHandle on_login!")
        accound_id = await self.__get_client_account_id__(sdk_uuid)
        info_str = app().redis_proxy.get("sample:player_info:{}".format(accound_id))
        print("LoginEventHandle on_login! redis_cache info_str:{}".format(info_str))
        if info_str:
            info = json.loads(info_str)
            print("LoginEventHandle on_login! info:{}".format(info))
            self.__replace_client__(info["gate"], info["conn_id"], new_gate_name, new_conn_id, "其他位置登录!")
        else:
            _p = SamplePlayer(str(uuid.uuid4()), new_gate_name, new_conn_id)
            app().player_mgr.add_player(_p)
            print("LoginEventHandle on_login! create_main_remote_entity:{} begin!".format(_p))
            _p.create_main_remote_entity()
            print("LoginEventHandle on_login! create_main_remote_entity:{}".format(_p))
    
    async def on_reconnect(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str):
        print("LoginEventHandle on_reconnect!")
        accound_id = await self.__get_client_account_id__(sdk_uuid)
        info_str = app().redis_proxy.get("sample:player_info:{}".format(accound_id))
        if info_str:
            info = json.loads(info_str)
            print("LoginEventHandle on_login! info:{}".format(info))
            self.__replace_client__(info["gate"], info["conn_id"], new_gate_name, new_conn_id, "其他位置登录!")
        else:
            _p = SamplePlayer(str(uuid.uuid4()), new_gate_name, new_conn_id)
            app().player_mgr.add_player(_p)
            print("LoginEventHandle on_login! create_main_remote_entity:{} begin!".format(_p))
            _p.create_main_remote_entity()
            rankImpl.call_update_rank(_p.entity_id)
            print("LoginEventHandle on_login! create_main_remote_entity:{}".format(_p))
    
class PlayerEventHandle(player_event_handle):
    def player_offline(self, _player:player) -> dict:
        return _player.info()
    
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