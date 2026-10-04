# -*- coding: UTF-8 -*-
from abc import ABC, abstractmethod
from collections.abc import Callable
import asyncio
import random
import msgpack

from .base_entity import base_entity
from .callback import callback

class player(ABC, base_entity):
    def __init__(self, service_name:str, entity_type:str, entity_id:str, gate_name:str, conn_id:str, is_dynamic:bool) -> None:
        base_entity.__init__(self, entity_type, entity_id)

        self.hub_request_callback:dict[str, Callable[[str, int, bytes],None]] = {}
        self.hub_notify_callback:dict[str, Callable[[str, bytes],None]] = {}
        
        self.request_msg_cb_id = random.randint(100, 10011)
        self.client_request_callback:dict[str, Callable[[str, str, int, bytes],None]] = {}
        self.client_notify_callback:dict[str, Callable[[str, bytes],None]] = {}

        self.client_callback:dict[int, callback] = {}

        self.service_name = service_name
        self.client_gate_name:str = gate_name
        self.client_conn_id:str = conn_id
        self.conn_hub_server:list[str] = []
        self.conn_client_gate:list[str] = []
        
        from .app import app

        self.is_dynamic = is_dynamic
        if is_dynamic:
            self.wait_lock_migrate_svr:list[str] = []
            from threading import Timer
            self.__migrate_timer__ = Timer(app().ctx.migrate_time_interval(), self.try_migrate_entity)
            self.__migrate_timer__.start()
        self.is_migrate = False

        app().player_mgr.add_player(self)

        app().trace("player __init__ end!")

    @abstractmethod
    def full_info(self) -> dict:
        pass
        
    @abstractmethod
    def hub_info(self) -> dict:
        pass

    @abstractmethod
    def client_info(self) -> dict:
        pass
    
    def on_migrate_to_other_hub(self):
        from .app import app
        app().entity_mgr.del_entity(self.entity_id)
        app().save_mgr.del_save_entity(self.entity_id)

    def on_transfer_conn(self, gate_name:str, conn_id:str):
        """客户端连接被转移到新的 gate/conn（换设备登录、掉线重连）后的回调，子类按需覆盖。"""
        pass

    def try_migrate_entity(self):
        if not self.is_dynamic:
            return
        from .app import app
        from threading import Timer
        if not app().is_idle:
            import random
            if random.random() < 0.2:
                app().run_coroutine_async(self.start_migrate_entity())
            else:
                self.__migrate_timer__ = Timer(app().ctx.migrate_time_interval(), self.try_migrate_entity)
                self.__migrate_timer__.start()

    async def start_migrate_entity(self):
        from .app import app
        migrate_hub = await app().ctx.entry_hub_service(self.service_name)
        await self.start_migrate_entity_initiative(migrate_hub)
            
    async def start_migrate_entity_initiative(self, migrate_hub:str):
        if migrate_hub != "":
            from .app import app
            app().ctx.hub_call_hub_migrate_entity(migrate_hub, self.service_name, self.entity_type, self.entity_id, "", "", self.conn_client_gate, self.conn_hub_server, msgpack.dumps(self.full_info()))

            for hub in self.conn_hub_server:
                app().ctx.hub_call_hub_wait_migrate_entity(hub, self.entity_id)
            for gate in self.conn_client_gate:
                app().ctx.hub_call_gate_wait_migrate_entity(gate, self.entity_id)
            self.is_migrate = True
            
            self.__migrate_timer__.cancel()
            
    async def migrate_entity_complete(self):
        from .app import app
        for hub in self.conn_hub_server:
            app().ctx.hub_call_hub_migrate_entity_complete(hub, self.entity_id)
        for gate in self.conn_client_gate:
            app().ctx.hub_call_gate_migrate_entity_complete(gate, self.entity_id)
        if self.client_gate_name not in self.conn_client_gate:
            app().ctx.hub_call_gate_migrate_entity_complete(self.client_gate_name, self.entity_id)
        self.on_migrate_to_other_hub()

    def create_main_remote_entity(self):
        try:
            from .app import app
            # 返回值是"hub → gate"这一跳有没有发出去（gate proxy 不存在 / 写失败时是 False）。
            # 以前直接丢掉返回值，客户端收不到 entity 时服务端没有任何日志，非常难查。
            if not app().ctx.hub_call_client_create_remote_entity(self.client_gate_name, self.is_migrate, [], self.client_conn_id, self.entity_id, self.entity_type, msgpack.dumps(self.client_info())):
                app().error(f"create_main_remote_entity faild send to gate! gate_name:{self.client_gate_name} conn_id:{self.client_conn_id} entity_id:{self.entity_id}")
        except Exception as e:
            app().trace(f"create_main_remote_entity err:{e}")

    def create_remote_entity(self, gate_name:str, conn_id:list[str]):
        if gate_name not in self.conn_client_gate:
            self.conn_client_gate.append(gate_name)
        from .app import app
        if not app().ctx.hub_call_client_create_remote_entity(gate_name, self.is_migrate, conn_id, "", self.entity_id, self.entity_type, msgpack.dumps(self.client_info())):
            app().error(f"create_remote_entity faild send to gate! gate_name:{gate_name} conn_id:{conn_id} entity_id:{self.entity_id}")
    
    def create_remote_hub_entity(self, hub_name:str):
        if hub_name not in self.conn_hub_server:
            self.conn_hub_server.append(hub_name)
        from .app import app
        app().ctx.create_service_entity(self.is_migrate, hub_name, self.service_name, self.entity_id, self.entity_type, msgpack.dumps(self.hub_info()))

    def handle_hub_request(self, source_hub:str, method:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.hub_request_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, msg_cb_id, argvs)
        else:
            self.error("unhandle request method:{}, source:{}".format(method, source_hub))

    def handle_hub_notify(self, source_hub:str, method:str, argvs:bytes):
        _call_handle = self.hub_notify_callback[method]
        if _call_handle != None:
            _call_handle(source_hub, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, source_hub))

    def reg_hub_request_callback(self, method:str, callback:Callable[[str, int, bytes],None]):
        self.hub_request_callback[method] = callback

    def reg_hub_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.hub_notify_callback[method] = callback

    def call_hub_response(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        from .app import app
        app().ctx.hub_call_hub_rsp(hub_name, self.entity_id, msg_cb_id, argvs)

    def call_hub_response_error(self, hub_name:str, msg_cb_id:int, argvs:bytes):
        from .app import app
        app().ctx.hub_call_hub_err(hub_name, self.entity_id, msg_cb_id, argvs)

    def call_hub_notify(self, method:str, argvs:bytes):
        from .app import app
        for hub_name in self.conn_hub_server:
            app().ctx.hub_call_hub_ntf(hub_name, self.entity_id, method, argvs)

    def handle_client_request(self, gate_name:str, conn_id:str, method:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_request_callback[method]
        if _call_handle != None:
            _call_handle(gate_name, conn_id, msg_cb_id, argvs)
        else:
            self.error("unhandle request method:{}, source:({}, {})".format(method, gate_name, conn_id))

    def del_client_callback(self, msg_cb_id:int) -> bool:
        if msg_cb_id not in self.client_callback:
            return False
        del self.client_callback[msg_cb_id]
        return True

    def handle_client_response(self, gate_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle._callback(argvs)
            del self.client_callback[msg_cb_id]
        else:
            self.error("unhandle response callback:{}, source:{}".format(msg_cb_id, gate_name))
    
    def handle_client_response_error(self, gate_name:str, msg_cb_id:int, argvs:bytes):
        _call_handle = self.client_callback[msg_cb_id]
        if _call_handle != None:
            _call_handle.error(argvs)
            del self.client_callback[msg_cb_id]
        else:
            self.error("unhandle response error callback:{}, source:{}".format(msg_cb_id, gate_name))

    def handle_client_notify(self, gate_name:str, method:str, argvs:bytes):
        _call_handle = self.client_notify_callback[method]
        if _call_handle != None:
            _call_handle(gate_name, argvs)
        else:
            self.error("unhandle notify method:{}, source:{}".format(method, gate_name))

    def reg_client_request_callback(self, method:str, callback:Callable[[str, str, int, bytes],None]):
        self.client_request_callback[method] = callback

    def reg_client_notify_callback(self, method:str, callback:Callable[[str, bytes],None]):
        self.client_notify_callback[method] = callback

    def call_client_request(self, method:str, argvs:bytes) -> int:
        from .app import app
        msg_cb_id = self.request_msg_cb_id
        self.request_msg_cb_id += 1
        app().ctx.hub_call_client_rpc(self.client_gate_name, self.entity_id, msg_cb_id, method, argvs)
        return msg_cb_id
        
    def reg_client_callback(self, msg_cb_id:int, rsp:callback):
        self.client_callback[msg_cb_id] = rsp

    def call_client_response(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from .app import app
        app().ctx.hub_call_client_rsp(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_response_error(self, gate_name:str, conn_id:str, msg_cb_id:int, argvs:bytes):
        from .app import app
        app().ctx.hub_call_client_err(gate_name, conn_id, self.entity_id, msg_cb_id, argvs)

    def call_client_main_notify(self, method:str, argvs:bytes):
        from .app import app
        app().ctx.hub_call_client_ntf(self.client_gate_name, self.client_conn_id, self.entity_id, method, argvs)

    def call_client_mutilcast(self, method:str, argvs:bytes):
        from .app import app
        for gate_name in self.conn_client_gate:
            app().ctx.hub_call_client_ntf(gate_name, "", self.entity_id, method, argvs)

class player_event_handle(ABC):
    @abstractmethod
    def player_offline(self, _player:player) -> dict:
        pass

class player_manager(object):
    def __init__(self, player_event_handle:player_event_handle):
        self.__player_event_handle__ = player_event_handle
        self.players:dict[str, player] = {}
        self.conn_id_players:dict[str, list[player]] = {}
        
    def add_player(self, _player:player):
        # 同一个 entity_id 出现新实例（重新登录 / 掉线重连 / 老会话还没清干净就重进）时，
        # 必须把旧实例从 conn_id_players 里摘掉：否则旧连接断开时 on_kick_off_client /
        # on_client_disconnnect 会按旧 conn_id 找到旧实例，把它（以及 scene.players、
        # group.players 里同 key 的新会话）一起当成离线踢掉，表现就是"第二次登录进去又没反应"。
        _old = self.players.get(_player.entity_id)
        if _old is not None and _old is not _player:
            self.__detach_player_conn__(_old)

        self.players[_player.entity_id] = _player

        if not _player.client_conn_id in self.conn_id_players:
            self.conn_id_players[_player.client_conn_id] = []
        if not _player in self.conn_id_players[_player.client_conn_id]:
            self.conn_id_players[_player.client_conn_id].append(_player)

    def __detach_player_conn__(self, _player:player):
        _p_list = self.conn_id_players.get(_player.client_conn_id)
        if _p_list is None:
            return
        if _player in _p_list:
            _p_list.remove(_player)
        if len(_p_list) <= 0:
            del self.conn_id_players[_player.client_conn_id]

    def get_player(self, entity_id:str) -> player:
        if entity_id in self.players:
            return self.players[entity_id]
        return None
    
    def update_player_conn(self, entity_id:str, is_main:bool, is_reconnect:bool, gate_name:str, conn_id:str) -> bool:
        _player = self.players.get(entity_id)
        if _player is None:
            return False

        _old_conn_id = _player.client_conn_id
        _p_list = self.conn_id_players.get(_old_conn_id, [])
        if _player not in _p_list:
            _p_list.append(_player)

        if gate_name not in _player.conn_client_gate:
            _player.conn_client_gate.append(gate_name)

        for _p in _p_list:
            if gate_name not in _p.conn_client_gate:
                _p.conn_client_gate.append(gate_name)

            _p.client_conn_id = conn_id
            _p.client_gate_name = gate_name
            _p.on_transfer_conn(gate_name, conn_id)

        # 旧的 conn_id 索引必须删掉：否则旧连接断开时 player_offline(旧conn_id) 仍然能在
        # conn_id_players 里找到这批玩家，把刚刚转移到新连接的玩家当成离线踢下线。
        if _old_conn_id != conn_id:
            self.conn_id_players.pop(_old_conn_id, None)
        self.conn_id_players[conn_id] = _p_list

        from .app import app
        if is_reconnect:
            if not app().ctx.hub_call_client_refresh_entity(gate_name, _player.is_migrate, conn_id, is_main, _player.entity_id, _player.entity_type, msgpack.dumps(_player.client_info())):
                app().error(f"update_player_conn refresh_entity faild send to gate! gate_name:{gate_name} conn_id:{conn_id} entity_id:{_player.entity_id}")
        else:
            if is_main:
                if not app().ctx.hub_call_client_create_remote_entity(gate_name, _player.is_migrate, [], conn_id, _player.entity_id, _player.entity_type, msgpack.dumps(_player.client_info())):
                    app().error(f"update_player_conn create_remote_entity faild send to gate! gate_name:{gate_name} conn_id:{conn_id} entity_id:{_player.entity_id}")
            else:
                if not app().ctx.hub_call_client_create_remote_entity(gate_name, _player.is_migrate, [conn_id], "", _player.entity_id, _player.entity_type, msgpack.dumps(_player.client_info())):
                    app().error(f"update_player_conn create_remote_entity faild send to gate! gate_name:{gate_name} conn_id:{conn_id} entity_id:{_player.entity_id}")
        
        return True
    
    def get_player_by_conn_id(self, conn_id:str) -> list[player]:
        if conn_id in self.conn_id_players:
            return self.conn_id_players[conn_id]
        return []
    
    def del_player(self, entity_id:str, _player:player = None):
        if entity_id not in self.players:
            return
        # 只有在册的就是这个实例时才删，避免把已经接管该 entity_id 的新会话删掉
        if _player is not None and self.players[entity_id] is not _player:
            return
        del self.players[entity_id]

    def del_player_list(self, conn_id:str):
         if conn_id in self.conn_id_players:
             del self.conn_id_players[conn_id]
        
    def player_offline(self, conn_id:str):
        _player_list = self.get_player_by_conn_id(conn_id)
        for _player in list(_player_list):
            # 玩家可能已经转移到新连接，或者该 entity_id 已经被新实例接管（重登/重连），
            # 旧连接的断开事件不能再动它，否则会把新会话一起踢掉。
            if _player.client_conn_id != conn_id:
                continue
            if self.players.get(_player.entity_id) is not _player:
                continue
            self.__player_event_handle__.player_offline(_player)
            self.del_player(_player.entity_id, _player)
        self.del_player_list(conn_id)
            
        