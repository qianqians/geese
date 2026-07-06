import sys, os, uuid

_dir = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_dir, 'engine'))

from engine.app import app
from engine.base_entity import client_event_handle
from engine.player import player
from engine.subentity import subentity
from engine.callback import callback

class ClientEventHandle(client_event_handle):
    def on_kick_off(self, prompt_info):
        print(prompt_info)
    def on_transfer_complete(self):
        print("on_transfer_complete")

playerImpl = None

class SamplePlayer(player):
    def __init__(self, entity_id):
        super().__init__("SamplePlayer", entity_id)
    def Creator(entity_id, description):
        global playerImpl
        playerImpl = SamplePlayer(entity_id)
        app().login(str(uuid.uuid4()), {})
        return playerImpl
    def update_player(self, argvs):
        print(f"SamplePlayer:{self.entity_id} update_player!")

def conn_callback(conn_id):
    app().login(str(uuid.uuid4()), {})

def main():
    _app = app()
    _app.build(ClientEventHandle())
    _app.register("SamplePlayer", SamplePlayer.Creator)
    _app.connect_ws("ws://127.0.0.1:8100", conn_callback)
    _app.run()

if __name__ == '__main__':
    main()
