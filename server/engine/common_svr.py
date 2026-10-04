from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *

# this enum code is codegen by geese codegen for python

class error_code(IntFlag):
    success = 0
    cannot_claimed = 1
    cannot_completed = 2
    not_enough_money = 3
    no_this_equip = 4
    no_this_item = 5
    undefined_player_id = 6
    unlock_talk_task = 7
    unlock_level_not_completed = 8
    unconfig_talk_task = 9
    talk_npc_not_scene = 10
    cannot_use_skill = 11
    not_in_spawn_point = 12


class em_rarity(IntFlag):
    common = 1
    rare = 2
    epic = 3
    legendary = 4
    myth = 5


class em_equip_type(IntFlag):
    helmet = 1
    jacket = 2
    trousers = 3
    gloves = 4
    boots = 5
    weapon = 6
    bb_attack = 11
    bb_defense = 12
    bb_resist = 13


class em_buff_type(IntFlag):
    em_seal_action = 1
    em_seal_skill = 2
    em_confuse_attack_all = 4
    em_confuse_attack_allies = 8
    em_heal_hp = 16
    em_heal_mp = 32
    em_damage_hp = 64
    em_damage_mp = 128
    em_defense_value = 256
    em_defense_ratio = 512


class direction(IntFlag):
    none = 0
    up = 1
    down = 2
    left = 4
    right = 8


class skill_type(IntFlag):
    skill_change_abonus_attack = 1
    skill_change_abonus_magic = 2
    skill_add_buffer = 3
    skill_dispel_buffer = 4


class em_task_state(IntFlag):
    can_claimed = 1
    in_progress = 2
    can_completed = 3
    completed = 4


class em_role_gender(IntFlag):
    em_role_gender_female = 0
    em_role_gender_male = 1


#this struct code is codegen by geese codegen for python
class attribute(object):
    def __init__(self):
        self.hp:int = 0
        self.mp:int = 0
        self.max_hp:int = 0
        self.max_mp:int = 0
        self.speed:int = 0
        self.attack:int = 0
        self.defense:int = 0
        self.tmp_defense:int = 0
        self.matk:int = 0
        self.resist:int = 0


def attribute_to_protcol(_struct:attribute):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["hp"] = _struct.hp
    _protocol["mp"] = _struct.mp
    _protocol["max_hp"] = _struct.max_hp
    _protocol["max_mp"] = _struct.max_mp
    _protocol["speed"] = _struct.speed
    _protocol["attack"] = _struct.attack
    _protocol["defense"] = _struct.defense
    _protocol["tmp_defense"] = _struct.tmp_defense
    _protocol["matk"] = _struct.matk
    _protocol["resist"] = _struct.resist
    return _protocol

def protcol_to_attribute(_protocol:dict):
    _struct = attribute()
    for (key, val) in _protocol.items():
        if key == "hp":
            _struct.hp = val
        elif key == "mp":
            _struct.mp = val
        elif key == "max_hp":
            _struct.max_hp = val
        elif key == "max_mp":
            _struct.max_mp = val
        elif key == "speed":
            _struct.speed = val
        elif key == "attack":
            _struct.attack = val
        elif key == "defense":
            _struct.defense = val
        elif key == "tmp_defense":
            _struct.tmp_defense = val
        elif key == "matk":
            _struct.matk = val
        elif key == "resist":
            _struct.resist = val
    return _struct

class equip_info(object):
    def __init__(self):
        self.equip_id:str = ""
        self.name:str = ""
        self.icon:str = ""
        self.desc:str = ""
        self.equip_type:em_equip_type = 0
        self.rarity:em_rarity = 0
        self.abonus:attribute = None


def equip_info_to_protcol(_struct:equip_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["equip_id"] = _struct.equip_id
    _protocol["name"] = _struct.name
    _protocol["icon"] = _struct.icon
    _protocol["desc"] = _struct.desc
    _protocol["equip_type"] = _struct.equip_type
    _protocol["rarity"] = _struct.rarity
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    return _protocol

def protcol_to_equip_info(_protocol:dict):
    _struct = equip_info()
    for (key, val) in _protocol.items():
        if key == "equip_id":
            _struct.equip_id = val
        elif key == "name":
            _struct.name = val
        elif key == "icon":
            _struct.icon = val
        elif key == "desc":
            _struct.desc = val
        elif key == "equip_type":
            _struct.equip_type = val
        elif key == "rarity":
            _struct.rarity = val
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
    return _struct

class position_info(object):
    def __init__(self):
        self.x:int = 0
        self.y:int = 0
        self.x_speed:int = 0
        self.y_speed:int = 0
        self.dir:int = 0


def position_info_to_protcol(_struct:position_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["x"] = _struct.x
    _protocol["y"] = _struct.y
    _protocol["x_speed"] = _struct.x_speed
    _protocol["y_speed"] = _struct.y_speed
    _protocol["dir"] = _struct.dir
    return _protocol

def protcol_to_position_info(_protocol:dict):
    _struct = position_info()
    for (key, val) in _protocol.items():
        if key == "x":
            _struct.x = val
        elif key == "y":
            _struct.y = val
        elif key == "x_speed":
            _struct.x_speed = val
        elif key == "y_speed":
            _struct.y_speed = val
        elif key == "dir":
            _struct.dir = val
    return _struct

class skill_info(object):
    def __init__(self):
        self.skill_id:int = 0
        self._type:skill_type = 0
        self.target_enemy:bool = False
        self.value0:float = 0.0
        self.value1:float = 0.0
        self.ratio:float = 0.0
        self.range:int = 0
        self.cast_mp:int = 0
        self.cd_round:int = 0


def skill_info_to_protcol(_struct:skill_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["skill_id"] = _struct.skill_id
    _protocol["_type"] = _struct._type
    _protocol["target_enemy"] = _struct.target_enemy
    _protocol["value0"] = _struct.value0
    _protocol["value1"] = _struct.value1
    _protocol["ratio"] = _struct.ratio
    _protocol["range"] = _struct.range
    _protocol["cast_mp"] = _struct.cast_mp
    _protocol["cd_round"] = _struct.cd_round
    return _protocol

def protcol_to_skill_info(_protocol:dict):
    _struct = skill_info()
    for (key, val) in _protocol.items():
        if key == "skill_id":
            _struct.skill_id = val
        elif key == "_type":
            _struct._type = val
        elif key == "target_enemy":
            _struct.target_enemy = val
        elif key == "value0":
            _struct.value0 = val
        elif key == "value1":
            _struct.value1 = val
        elif key == "ratio":
            _struct.ratio = val
        elif key == "range":
            _struct.range = val
        elif key == "cast_mp":
            _struct.cast_mp = val
        elif key == "cd_round":
            _struct.cd_round = val
    return _struct

class gongfa(object):
    def __init__(self):
        self.gongfa_id:str = ""
        self.rarity:em_rarity = 0
        self.gongfa_level:int = 0
        self.abonus:attribute = None
        self.skills:list[skill_info] = []


def gongfa_to_protcol(_struct:gongfa):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["gongfa_id"] = _struct.gongfa_id
    _protocol["rarity"] = _struct.rarity
    _protocol["gongfa_level"] = _struct.gongfa_level
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    if _struct.skills:
        _array_skills = []
        for v_ in _struct.skills:
            _array_skills.append(skill_info_to_protcol(v_))
        _protocol["skills"] = _array_skills
    return _protocol

def protcol_to_gongfa(_protocol:dict):
    _struct = gongfa()
    for (key, val) in _protocol.items():
        if key == "gongfa_id":
            _struct.gongfa_id = val
        elif key == "rarity":
            _struct.rarity = val
        elif key == "gongfa_level":
            _struct.gongfa_level = val
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
        elif key == "skills":
            _struct.skills = []
            for v_ in val:
                _struct.skills.append(skill_info_to_protcol(v_))
    return _struct

class task_progress_info(object):
    def __init__(self):
        self.table_id:int = 0
        self.total:int = 0
        self.progress:int = 0
        self.watch_task:list[int] = []


def task_progress_info_to_protcol(_struct:task_progress_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["table_id"] = _struct.table_id
    _protocol["total"] = _struct.total
    _protocol["progress"] = _struct.progress
    if _struct.watch_task:
        _array_watch_task = []
        for v_ in _struct.watch_task:
            _array_watch_task.append(v_)
        _protocol["watch_task"] = _array_watch_task
    return _protocol

def protcol_to_task_progress_info(_protocol:dict):
    _struct = task_progress_info()
    for (key, val) in _protocol.items():
        if key == "table_id":
            _struct.table_id = val
        elif key == "total":
            _struct.total = val
        elif key == "progress":
            _struct.progress = val
        elif key == "watch_task":
            _struct.watch_task = []
            for v_ in val:
                _struct.watch_task.append(v_)
    return _struct

class task_info(object):
    def __init__(self):
        self.task_id:int = 0
        self.status:em_task_state = 0
        self.progress:list[task_progress_info] = []
        self.refresh_time:int = 0


def task_info_to_protcol(_struct:task_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["task_id"] = _struct.task_id
    _protocol["status"] = _struct.status
    if _struct.progress:
        _array_progress = []
        for v_ in _struct.progress:
            _array_progress.append(task_progress_info_to_protcol(v_))
        _protocol["progress"] = _array_progress
    _protocol["refresh_time"] = _struct.refresh_time
    return _protocol

def protcol_to_task_info(_protocol:dict):
    _struct = task_info()
    for (key, val) in _protocol.items():
        if key == "task_id":
            _struct.task_id = val
        elif key == "status":
            _struct.status = val
        elif key == "progress":
            _struct.progress = []
            for v_ in val:
                _struct.progress.append(task_progress_info_to_protcol(v_))
        elif key == "refresh_time":
            _struct.refresh_time = val
    return _struct

class item(object):
    def __init__(self):
        self.item_id:str = ""
        self.item_type:int = 0
        self.item_count:int = 0


def item_to_protcol(_struct:item):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["item_id"] = _struct.item_id
    _protocol["item_type"] = _struct.item_type
    _protocol["item_count"] = _struct.item_count
    return _protocol

def protcol_to_item(_protocol:dict):
    _struct = item()
    for (key, val) in _protocol.items():
        if key == "item_id":
            _struct.item_id = val
        elif key == "item_type":
            _struct.item_type = val
        elif key == "item_count":
            _struct.item_count = val
    return _struct

class bb(object):
    def __init__(self):
        self.entity_id:str = ""
        self.bb_table_id:int = 0
        self.rarity:em_rarity = 0
        self.level:int = 0
        self.abonus:attribute = None
        self.skills:list[skill_info] = []
        self.equips:list[equip_info] = []


def bb_to_protcol(_struct:bb):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["entity_id"] = _struct.entity_id
    _protocol["bb_table_id"] = _struct.bb_table_id
    _protocol["rarity"] = _struct.rarity
    _protocol["level"] = _struct.level
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    if _struct.skills:
        _array_skills = []
        for v_ in _struct.skills:
            _array_skills.append(skill_info_to_protcol(v_))
        _protocol["skills"] = _array_skills
    if _struct.equips:
        _array_equips = []
        for v_ in _struct.equips:
            _array_equips.append(equip_info_to_protcol(v_))
        _protocol["equips"] = _array_equips
    return _protocol

def protcol_to_bb(_protocol:dict):
    _struct = bb()
    for (key, val) in _protocol.items():
        if key == "entity_id":
            _struct.entity_id = val
        elif key == "bb_table_id":
            _struct.bb_table_id = val
        elif key == "rarity":
            _struct.rarity = val
        elif key == "level":
            _struct.level = val
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
        elif key == "skills":
            _struct.skills = []
            for v_ in val:
                _struct.skills.append(skill_info_to_protcol(v_))
        elif key == "equips":
            _struct.equips = []
            for v_ in val:
                _struct.equips.append(equip_info_to_protcol(v_))
    return _struct

class partner(object):
    def __init__(self):
        self.entity_id:str = ""
        self.partner_table_id:int = 0
        self.rarity:em_rarity = 0
        self.curr_gf:gongfa = None
        self.abonus:attribute = None


def partner_to_protcol(_struct:partner):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["entity_id"] = _struct.entity_id
    _protocol["partner_table_id"] = _struct.partner_table_id
    _protocol["rarity"] = _struct.rarity
    _protocol["curr_gf"] = gongfa_to_protcol(_struct.curr_gf)
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    return _protocol

def protcol_to_partner(_protocol:dict):
    _struct = partner()
    for (key, val) in _protocol.items():
        if key == "entity_id":
            _struct.entity_id = val
        elif key == "partner_table_id":
            _struct.partner_table_id = val
        elif key == "rarity":
            _struct.rarity = val
        elif key == "curr_gf":
            _struct.curr_gf = protcol_to_gongfa(val)
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
    return _struct

class player_info(object):
    def __init__(self):
        self.account_id:str = ""
        self.player_id:str = ""
        self.player_nick_name:str = ""
        self.player_appearance:str = ""
        self.abonus:attribute = None
        self.gfs:list[gongfa] = []
        self.curr_gf:gongfa = None
        self.equips:list[equip_info] = []
        self.wait_bbs:list[bb] = []
        self.curr_bbs:list[bb] = []
        self.wait_partner:list[partner] = []
        self.curr_partner:list[partner] = []
        self.items:list[item] = []
        self.tasks:list[task_info] = []
        self.gender:em_role_gender = 0
        self.scene:str = ""
        self.line:int = 0
        self.pos:position_info = None


def player_info_to_protcol(_struct:player_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["account_id"] = _struct.account_id
    _protocol["player_id"] = _struct.player_id
    _protocol["player_nick_name"] = _struct.player_nick_name
    _protocol["player_appearance"] = _struct.player_appearance
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    if _struct.gfs:
        _array_gfs = []
        for v_ in _struct.gfs:
            _array_gfs.append(gongfa_to_protcol(v_))
        _protocol["gfs"] = _array_gfs
    _protocol["curr_gf"] = gongfa_to_protcol(_struct.curr_gf)
    if _struct.equips:
        _array_equips = []
        for v_ in _struct.equips:
            _array_equips.append(equip_info_to_protcol(v_))
        _protocol["equips"] = _array_equips
    if _struct.wait_bbs:
        _array_wait_bbs = []
        for v_ in _struct.wait_bbs:
            _array_wait_bbs.append(bb_to_protcol(v_))
        _protocol["wait_bbs"] = _array_wait_bbs
    if _struct.curr_bbs:
        _array_curr_bbs = []
        for v_ in _struct.curr_bbs:
            _array_curr_bbs.append(bb_to_protcol(v_))
        _protocol["curr_bbs"] = _array_curr_bbs
    if _struct.wait_partner:
        _array_wait_partner = []
        for v_ in _struct.wait_partner:
            _array_wait_partner.append(partner_to_protcol(v_))
        _protocol["wait_partner"] = _array_wait_partner
    if _struct.curr_partner:
        _array_curr_partner = []
        for v_ in _struct.curr_partner:
            _array_curr_partner.append(partner_to_protcol(v_))
        _protocol["curr_partner"] = _array_curr_partner
    if _struct.items:
        _array_items = []
        for v_ in _struct.items:
            _array_items.append(item_to_protcol(v_))
        _protocol["items"] = _array_items
    if _struct.tasks:
        _array_tasks = []
        for v_ in _struct.tasks:
            _array_tasks.append(task_info_to_protcol(v_))
        _protocol["tasks"] = _array_tasks
    _protocol["gender"] = _struct.gender
    _protocol["scene"] = _struct.scene
    _protocol["line"] = _struct.line
    _protocol["pos"] = position_info_to_protcol(_struct.pos)
    return _protocol

def protcol_to_player_info(_protocol:dict):
    _struct = player_info()
    for (key, val) in _protocol.items():
        if key == "account_id":
            _struct.account_id = val
        elif key == "player_id":
            _struct.player_id = val
        elif key == "player_nick_name":
            _struct.player_nick_name = val
        elif key == "player_appearance":
            _struct.player_appearance = val
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
        elif key == "gfs":
            _struct.gfs = []
            for v_ in val:
                _struct.gfs.append(gongfa_to_protcol(v_))
        elif key == "curr_gf":
            _struct.curr_gf = protcol_to_gongfa(val)
        elif key == "equips":
            _struct.equips = []
            for v_ in val:
                _struct.equips.append(equip_info_to_protcol(v_))
        elif key == "wait_bbs":
            _struct.wait_bbs = []
            for v_ in val:
                _struct.wait_bbs.append(bb_to_protcol(v_))
        elif key == "curr_bbs":
            _struct.curr_bbs = []
            for v_ in val:
                _struct.curr_bbs.append(bb_to_protcol(v_))
        elif key == "wait_partner":
            _struct.wait_partner = []
            for v_ in val:
                _struct.wait_partner.append(partner_to_protcol(v_))
        elif key == "curr_partner":
            _struct.curr_partner = []
            for v_ in val:
                _struct.curr_partner.append(partner_to_protcol(v_))
        elif key == "items":
            _struct.items = []
            for v_ in val:
                _struct.items.append(item_to_protcol(v_))
        elif key == "tasks":
            _struct.tasks = []
            for v_ in val:
                _struct.tasks.append(task_info_to_protcol(v_))
        elif key == "gender":
            _struct.gender = val
        elif key == "scene":
            _struct.scene = val
        elif key == "line":
            _struct.line = val
        elif key == "pos":
            _struct.pos = protcol_to_position_info(val)
    return _struct

class battle_entity(object):
    def __init__(self):
        self.entity_id:str = ""
        self.nick_name:str = ""
        self.appearance:str = ""
        self.speed:int = 0
        self.level:int = 0
        self.abonus:attribute = None
        self.skills:list[skill_info] = []


def battle_entity_to_protcol(_struct:battle_entity):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["entity_id"] = _struct.entity_id
    _protocol["nick_name"] = _struct.nick_name
    _protocol["appearance"] = _struct.appearance
    _protocol["speed"] = _struct.speed
    _protocol["level"] = _struct.level
    _protocol["abonus"] = attribute_to_protcol(_struct.abonus)
    if _struct.skills:
        _array_skills = []
        for v_ in _struct.skills:
            _array_skills.append(skill_info_to_protcol(v_))
        _protocol["skills"] = _array_skills
    return _protocol

def protcol_to_battle_entity(_protocol:dict):
    _struct = battle_entity()
    for (key, val) in _protocol.items():
        if key == "entity_id":
            _struct.entity_id = val
        elif key == "nick_name":
            _struct.nick_name = val
        elif key == "appearance":
            _struct.appearance = val
        elif key == "speed":
            _struct.speed = val
        elif key == "level":
            _struct.level = val
        elif key == "abonus":
            _struct.abonus = protcol_to_attribute(val)
        elif key == "skills":
            _struct.skills = []
            for v_ in val:
                _struct.skills.append(skill_info_to_protcol(v_))
    return _struct

class battle_info(object):
    def __init__(self):
        self.battle_team0:battle_entity = None
        self.battle_team1:battle_entity = None
        self.curr_bbs0:bb = None
        self.curr_bbs1:bb = None
        self.player:battle_entity = None
        self.items:list[item] = []
        self.scene:str = ""


def battle_info_to_protcol(_struct:battle_info):
    if _struct is None:
        return None
    _protocol = {}
    _protocol["battle_team0"] = battle_entity_to_protcol(_struct.battle_team0)
    _protocol["battle_team1"] = battle_entity_to_protcol(_struct.battle_team1)
    _protocol["curr_bbs0"] = bb_to_protcol(_struct.curr_bbs0)
    _protocol["curr_bbs1"] = bb_to_protcol(_struct.curr_bbs1)
    _protocol["player"] = battle_entity_to_protcol(_struct.player)
    if _struct.items:
        _array_items = []
        for v_ in _struct.items:
            _array_items.append(item_to_protcol(v_))
        _protocol["items"] = _array_items
    _protocol["scene"] = _struct.scene
    return _protocol

def protcol_to_battle_info(_protocol:dict):
    _struct = battle_info()
    for (key, val) in _protocol.items():
        if key == "battle_team0":
            _struct.battle_team0 = protcol_to_battle_entity(val)
        elif key == "battle_team1":
            _struct.battle_team1 = protcol_to_battle_entity(val)
        elif key == "curr_bbs0":
            _struct.curr_bbs0 = protcol_to_bb(val)
        elif key == "curr_bbs1":
            _struct.curr_bbs1 = protcol_to_bb(val)
        elif key == "player":
            _struct.player = protcol_to_battle_entity(val)
        elif key == "items":
            _struct.items = []
            for v_ in val:
                _struct.items.append(item_to_protcol(v_))
        elif key == "scene":
            _struct.scene = val
    return _struct

#this caller code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python

