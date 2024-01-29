import sys
import uuid
from engine.engine import *
from engine.login_cli import *
from engine.get_rank_cli import *

class ClientEventHandle(client_event_handle):
    def on_kick_off(self, prompt_info:str):
        print(prompt_info)

    def on_transfer_complete(self):
        print("on_transfer_complete")
        
playerImpl = None

class RankSubEntity(subentity):
    def __init__(self, entity_type: str, entity_id: str) -> None:
        super().__init__(entity_type, entity_id)
        self.get_rank_caller = get_rank_caller(self)

    def get_self_rank(self, entity_id):
        self.get_rank_caller.get_self_rank(entity_id).callBack(
            lambda _info: print(f"RankSubEntity get_self_rank callBack:{_info}"),
            lambda _err: print(f"RankSubEntity get_self_rank err:{_err}")).timeout(
                1000, lambda: print(f"RankSubEntity get_self_rank timeout!"))

    def update_subentity(self, argvs: dict):
        print(f"RankSubEntity:{self.entity_id} update_subentity!")

    def Creator(entity_id:str, description: dict):
        print(f"RankSubEntity Creator entity_id:{entity_id}")
        rankImpl = RankSubEntity("RankImpl", entity_id)
        rankImpl.get_self_rank(playerImpl.entity_id)
        return rankImpl

class SamplePlayer(player):
    def __init__(self, entity_id: str):
        super().__init__("SamplePlayer", entity_id)
        self.login_caller = login_caller(self)
        
    def Creator(entity_id: str, description: dict):
        print(f"SamplePlayer:{entity_id}")
        global playerImpl
        playerImpl = SamplePlayer(entity_id)
        playerImpl.login_caller.login("entity_id-123456").callBack(
            lambda success: print(f"SamplePlayer login success:{success}"),
            lambda _err: print(f"SamplePlayer login _err:{_err}")).timeout(
            1000, lambda: print(f"SamplePlayer login timeout!"))
        app().request_hub_service("Rank")
        return playerImpl
            
    def update_player(self, argvs: dict):
        print(f"SamplePlayer:{self.entity_id} update_player!")

def conn_callback(conn_id:str):
    print("conn_callback begin!")
    app().login(str(uuid.uuid4()))
    print("conn_callback end!")

def main():
    _app = app()
    _app.build(ClientEventHandle())
    _app.register("SamplePlayer", SamplePlayer.Creator)
    _app.register("RankImpl", RankSubEntity.Creator)
    _app.connect_tcp("127.0.0.1", 8000, conn_callback)
    print(f"run begin!")
    _app.run()
    
if __name__ == '__main__':
    main()