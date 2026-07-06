import sys
from engine.engine import *
from engine.update_rank_svr import *

class RankEntity(entity):
    def __init__(self, entity_id: str):
        super().__init__("Rank", "RankImpl", entity_id, False)
        self.update_rank_module = update_rank_module(self)
        self.update_rank_module.on_call_update_rank.append(self.on_call_update_rank)

    def on_call_update_rank(self, rsp: update_rank_call_update_rank_rsp, entity_id: str):
        app().trace(f"RankEntity on_call_update_rank entity_id:{entity_id}")
        rsp.rsp()

    def full_info(self) -> dict:
        return {}

    def hub_info(self) -> dict:
        return {}

    def client_info(self) -> dict:
        return {}

    def on_migrate_to_other_hub(self, migrate_hub: str):
        pass

def Creator(is_migrate: bool, source_hub_name: str, entity_id: str, description: dict):
    app().trace(f"RankEntity Creator entity_id:{entity_id}")
    return RankEntity(entity_id)

def main(cfg_file: str):
    _app = app()
    _app.build(cfg_file)
    _app.register("RankImpl", Creator)
    _app.register_service("Rank")
    _app.run()

if __name__ == '__main__':
    main(sys.argv[1])
