import sys
from engine.engine import *
from engine.login_svr import *
from engine.update_rank_svr import *
from engine.get_rank_svr import *

class RankImpl(entity):
    def __init__(self):
        super().__init__("RankImpl", str(uuid.uuid4()))
        self.update_rank_module = update_rank_module(self)
        self.get_rank_module = get_rank_module(self)

        self.update_rank_module.on_call_update_rank.append(lambda rsp, entity_id: self.on_call_update_rank(rsp, entity_id))
        self.get_rank_module.on_get_self_rank.append(lambda rsp, entity_id: self.on_get_self_rank(rsp, entity_id))

    def hub_info(self) -> dict:
        return {}
    
    def client_info(self) -> dict:
        return {}
    
    def on_call_update_rank(self, rsp:update_rank_call_update_rank_rsp, _entity_id:str):
        app().trace(f"RankImpl on_call_update_rank _entity_id:{_entity_id}")
        rsp.rsp()

    def on_get_self_rank(self, rsp:get_rank_get_self_rank_rsp, _entity_id:str):
        app().trace(f"RankImpl on_get_self_rank _entity_id:{_entity_id}")
        rankInfo = role_rank_info()
        rankInfo.entity_id = _entity_id
        rankInfo.rank = 1
        rankInfo.role_name = "test"
        rsp.rsp(rankInfo)

@ServiceDescribe("Rank")
class RankService(service):
    def __init__(self, _app:app):
        super().__init__()
        self._app = _app
        self.rankImpl = RankImpl()
        self._app.entity_mgr.add_entity(self.rankImpl)

    def hub_query_service_entity(self, queryer_hub_name:str):
        self.rankImpl.create_remote_hub_entity(queryer_hub_name, "Rank")
    
    def client_query_service_entity(self, queryer_gate_name:str, queryer_client_conn_id:str):
        self.rankImpl.create_remote_entity(queryer_gate_name, queryer_client_conn_id)
    
class PlayerEventHandle(player_event_handle):
    def player_offline(self, _player:player) -> dict:
        return _player.info()
    
def main(cfg_file:str):
    _app = app()
    _app.build(cfg_file)
    _app.build_player_service(PlayerEventHandle())
    _app.service_mgr.reg_service(RankService(_app))
    _app.run()
    
if __name__ == '__main__':
    main(sys.argv[1])
