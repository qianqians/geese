import sys
import uuid
from engine.engine import *
from engine.login_cli import *

class ClientEventHandle(client_event_handle):
    def on_kick_off(self, prompt_info:str):
        print(prompt_info)

    def on_transfer_complete(self):
        print("on_transfer_complete")
        
class SamplePlayer(player):
    def __init__(self, entity_id: str):
        super().__init__("SamplePlayer", entity_id)
        self.login_caller = login_caller(self)
        
    def Creator(entity_id: str, description: dict):
        print(f"SamplePlayer:{entity_id}")
        _player = SamplePlayer(entity_id)
        _player.login_caller.login("entity_id-123456").callBack(
            lambda success: print(f"SamplePlayer login success:{success}"),
            lambda _err: print(f"SamplePlayer login _err:{_err}")).timeout(
            lambda: print(f"SamplePlayer login timeout!"))

def conn_callback(conn_id:str):
    print("conn_callback begin!")
    app().login(str(uuid.uuid4()))
    print("conn_callback end!")

def main():
    _app = app()
    _app.build(ClientEventHandle())
    _app.register("SamplePlayer", SamplePlayer.Creator)
    _app.connect_tcp("127.0.0.1", 8000, conn_callback)
    print(f"run begin!")
    _app.run()
    
if __name__ == '__main__':
    main()