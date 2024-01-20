import sys
from engine.engine import *

class SamplePlayer(player):
    def __init__(self, entity_id: str, gate_name: str, conn_id: str):
        player.__init__(self, "SamplePlayer", entity_id, gate_name, conn_id)
        
    def info(self) -> dict:
        return {}

class LoginEventHandle(login_event_handle):
    def __init__(self, db:str, collection:str):
        super().__init__(db, collection)
        
        self.__get_guid_handle__ = get_guid("sample", "account_uuid")

    def __callback_get_account__(self, uuidObj:dict, err:DBExtensionError, new_gate_name:str, new_conn_id, callback:Callable[[int, str, str],None]):
        if err:
            print(f"__callback_get_account__ error _db:{self.__db__}, collection:{self.__collection__}")

        print(f"LoginEventHandle __get_client_account_id__ uuidObj:{uuidObj}!")
        if not uuidObj:
            self.__get_guid_handle__.gen(lambda guid: callback(guid, new_gate_name, new_conn_id))
        else:
            callback(uuidObj["GUID"], new_gate_name, new_conn_id)

    def __get_client_account_id__(self, sdk_uuid:str, new_gate_name:str, new_conn_id:str, callback:Callable[[int, str, str],None]):
        print("LoginEventHandle __get_client_account_id__!")
        self.__get_dbproxy__().get_object_one(
            self.__db__, self.__collection__, {"SDK_UUID":sdk_uuid}, 
            lambda uuidObj, err: self.__callback_get_account__(uuidObj, err, new_gate_name, new_conn_id, callback))
        
    def __accound_id_callback__(self, accound_id:int, new_gate_name:str, new_conn_id:str):
        print("LoginEventHandle on_login! accound_id:{}".format(accound_id))
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

    def on_login(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str):
        print("LoginEventHandle on_login!")
        self.__get_client_account_id__(sdk_uuid, new_gate_name, new_conn_id, self.__accound_id_callback__)
    
    def on_reconnect(self, new_gate_name:str, new_conn_id:str, sdk_uuid:str, token:str):
        print("LoginEventHandle on_reconnect!")
        self.__get_client_account_id__(sdk_uuid, new_gate_name, new_conn_id, self.__accound_id_callback__)
    
class PlayerEventHandle(player_event_handle):
    def player_offline(self, _player:player) -> dict:
        return _player.info()
    
def main(cfg_file:str):
    _app = app()
    _app.build(cfg_file)
    _app.build_login_service(LoginEventHandle("sample", "account"))
    _app.build_player_service(PlayerEventHandle())
    _app.register_service("login")
    _app.run()
    
if __name__ == '__main__':
    main(sys.argv[1])