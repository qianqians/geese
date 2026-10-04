from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
class harm(object):
    def __init__(self):
        self.be_attack_entity_id:str = ""
        self.skill_id:int = 0
        self.harm_value:int = 0
        self.is_dead:bool = False


def harm_to_protcol(_struct:harm):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["be_attack_entity_id"] = _struct.be_attack_entity_id
    _protocol["skill_id"] = _struct.skill_id
    _protocol["harm_value"] = _struct.harm_value
    _protocol["is_dead"] = _struct.is_dead
    return _protocol

def protcol_to_harm(_protocol:dict):
    _struct = harm()
    for (key, val) in _protocol.items():
        if key == "be_attack_entity_id":
            _struct.be_attack_entity_id = val
        elif key == "skill_id":
            _struct.skill_id = val
        elif key == "harm_value":
            _struct.harm_value = val
        elif key == "is_dead":
            _struct.is_dead = val
    return _struct

#this caller code is codegen by geese codegen for python
class battle_ntf_client_caller(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

    def start_battle(self, self_side:battle_info, enemy:battle_info):
        _argv_01e120b2_ff3e_35bc_b812_e0d6fa294873 = []
        _argv_01e120b2_ff3e_35bc_b812_e0d6fa294873.append(battle_info_to_protcol(self_side))
        _argv_01e120b2_ff3e_35bc_b812_e0d6fa294873.append(battle_info_to_protcol(enemy))
        self.entity.call_client_mutilcast("start_battle", dumps(_argv_01e120b2_ff3e_35bc_b812_e0d6fa294873))

    def use_skill(self, caster_entity_id:str, hits:list[harm]):
        _argv_f54ecac1_af9c_3003_a2f2_ed93134bfdfe = []
        _argv_f54ecac1_af9c_3003_a2f2_ed93134bfdfe.append(caster_entity_id)
        _list_3b80b646_eaae_3eb3_b5db_41773fbe433a = []
        for v_84c0e237_1395_5b20_b7fa_478a639ad4b4 in hits:
            _list_3b80b646_eaae_3eb3_b5db_41773fbe433a.append(harm_to_protcol(v_84c0e237_1395_5b20_b7fa_478a639ad4b4))
        _argv_f54ecac1_af9c_3003_a2f2_ed93134bfdfe.append(_list_3b80b646_eaae_3eb3_b5db_41773fbe433a)
        self.entity.call_client_mutilcast("use_skill", dumps(_argv_f54ecac1_af9c_3003_a2f2_ed93134bfdfe))

    def battle_victory(self):
        _argv_5388fb35_f021_358e_992c_9d18e0f4cfc5 = []
        self.entity.call_client_mutilcast("battle_victory", dumps(_argv_5388fb35_f021_358e_992c_9d18e0f4cfc5))

    def battle_failed(self):
        _argv_3a3297ef_3cc5_3ec2_85f5_32b109e00b66 = []
        self.entity.call_client_mutilcast("battle_failed", dumps(_argv_3a3297ef_3cc5_3ec2_85f5_32b109e00b66))

    def battle_continue(self):
        _argv_029f0550_6dad_3479_82ba_2e7085411cc5 = []
        self.entity.call_client_mutilcast("battle_continue", dumps(_argv_029f0550_6dad_3479_82ba_2e7085411cc5))



