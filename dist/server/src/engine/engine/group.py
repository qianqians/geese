# -*- coding: UTF-8 -*-
from .player import *
from .entity import *

class group(object):
    def __init__(self):
        self.clients:list[tuple[str, str]] = [] 
        self.entities:dict[str, entity] = {}
        self.players:dict[str, player] = {}

    def join(self, client:tuple[str, str]):
        self.clients.append(client)

        gate_name, conn_id = client
        for _e in self.entities.values():
            _e.create_remote_entity(gate_name, [conn_id])
        for _p in self.players.values():
            _p.create_remote_entity(gate_name, [conn_id])

    def leave(self, client:tuple[str, str]):
        cli_gate_name, cli_conn_id = client
        for _c in self.clients:
            gate_name, conn_id = _c
            if cli_gate_name == gate_name and cli_conn_id == conn_id:
                self.clients.remove(_c)
                break
    
    def create_remote_entity(self, _e:entity):
        self.entities[_e.entity_id] = _e
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients:
            gate_name, conn_id = _c
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)
        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            _e.create_remote_entity(gate_name, list_conn_id)

    def remove_entity(self, _e:entity):   
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients:
            gate_name, conn_id = _c
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)

        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            from app import app
            app().ctx.hub_call_client_delete_remote_entity(gate_name, _e.entity_id)

        del self.entities[_e.entity_id]
        
    def create_remote_player(self, _p:player):
        self.players[_p.entity_id] = _p
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients:
            gate_name, conn_id = _c
            if conn_id == _p.client_conn_id:
                continue
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)
        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            _p.create_remote_entity(gate_name, list_conn_id)
        _p.create_main_remote_entity()
        
    def remove_player(self, _p:player):   
        gate_clients:dict[str, list[str]] = {}
        for _c in self.clients:
            gate_name, conn_id = _c
            if gate_name not in gate_clients:
                gate_clients[gate_name] = [conn_id]
            else:
                gate_clients[gate_name].append(conn_id)

        for _conn in gate_clients.items():
            gate_name, list_conn_id = _conn
            from app import app
            app().ctx.hub_call_client_delete_remote_entity(gate_name, _p.entity_id)

        del self.players[_p.entity_id]

