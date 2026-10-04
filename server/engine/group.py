# -*- coding: UTF-8 -*-
from .player import *
from .entity import *

class group(object):
    def __init__(self):
        self.clients:dict[str, tuple[str, str]] = {} 
        self.entities:dict[str, entity] = {}
        self.players:dict[str, player] = {}
        self.scene_object_ids:set[str] = set()
        self.scene_manifest_path:str|None = None
        self.dynamic_cache: dict[str, bytes] = {}

    def join(self, entity_id:str, client:tuple[str, str]):
        from .app import app
        gate_name, conn_id = client
        app().trace(f"group join! gate_name:{gate_name} conn_id:{conn_id}")
        for _e in self.entities.values():
            _e.create_remote_entity(gate_name, [conn_id])
        for _p in self.players.values():
            if _p.entity_id == entity_id:
                continue
            _p.create_remote_entity(gate_name, [conn_id])

        # 静态场景对象：重新同步给新客户端
        if self.scene_object_ids and self.scene_manifest_path:
            from scene_physics import sync_scene_to_group
            sync_scene_to_group(self.scene_manifest_path, self, gate_name, [conn_id])

        # 动态场景对象：重新同步给新客户端
        if self.dynamic_cache:
            from scene_physics import ENTITY_TYPE_DYNAMIC
            #from .app import app
            for entity_id, msg_bytes in self.dynamic_cache.items():
                app().ctx.hub_call_client_create_remote_entity(
                    gate_name,
                    False,  # is_migrate
                    [conn_id],
                    "",  # main_conn_id
                    entity_id,
                    ENTITY_TYPE_DYNAMIC,
                    msg_bytes)
        
        self.clients[entity_id] = client

    def leave(self, client:tuple[str, str]):
        cli_gate_name, cli_conn_id = client
        for entity_id, (gate_name, conn_id) in self.clients.items():
            if cli_gate_name == gate_name and cli_conn_id == conn_id:
                del self.clients[entity_id]

                from .app import app
                for _e in self.entities.values():
                    app().ctx.hub_call_client_remove_remote_entity(cli_gate_name, _e.entity_id, cli_conn_id)
                for _p in self.players.values():
                    app().ctx.hub_call_client_remove_remote_entity(cli_gate_name, _p.entity_id, cli_conn_id)
                for _sid in self.scene_object_ids:
                    app().ctx.hub_call_client_remove_remote_entity(cli_gate_name, _sid, cli_conn_id)
                break
    
    def create_remote_entity(self, _e:entity):
        self.entities[_e.entity_id] = _e
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients.items():
            _, (gate_name, conn_id) = _c
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)
        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            _e.create_remote_entity(gate_name, list_conn_id)

    def remove_entity(self, _e:entity):   
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients.items():
            _, (gate_name, conn_id) = _c
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)

        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            from .app import app
            app().ctx.hub_call_client_delete_remote_entity(gate_name, _e.entity_id)

        del self.entities[_e.entity_id]
        
    def create_remote_player(self, _p:player):
        self.players[_p.entity_id] = _p
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients.items():
            entity_id, (gate_name, conn_id) = _c
            if conn_id == _p.client_conn_id:
                continue
            if entity_id == _p.entity_id:
                continue
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)
        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            from .app import app
            app().trace(f"group create_remote_player! gate_name:{gate_name} list_conn_id:{list_conn_id}")
            _p.create_remote_entity(gate_name, list_conn_id)
        _p.create_main_remote_entity()
        
    def remove_player(self, _p:player):   
        # 组里登记的如果已经不是这个实例（该 entity_id 已被新实例接管），不要再删，
        # 否则会把新会话在 group 里的登记一起抹掉（新客户端从此收不到任何同步）。
        if self.players.get(_p.entity_id) is not _p:
            return

        gate_clients:list[str] = []
        for _, (gate_name, conn_id) in self.clients.items():
            if gate_name not in gate_clients:
                gate_clients.append(gate_name)

        for gate_name in gate_clients:
            from .app import app
            app().ctx.hub_call_client_delete_remote_entity(gate_name, _p.entity_id)

        del self.players[_p.entity_id]
        del self.clients[_p.entity_id]

