from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .physics_common_cli import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this module code is codegen by geese codegen for python
class physics_sync_module(object):
    def __init__(self, entity:player|subentity|receiver):
        self.entity = entity

        self.on_sync_bodies:list[Callable[[session, list[body_snapshot]], None]] = []
        self.entity.reg_hub_notify_callback("sync_bodies", self.sync_bodies)
        self.on_sync_contacts:list[Callable[[session, list[contact_info]], None]] = []
        self.entity.reg_hub_notify_callback("sync_contacts", self.sync_contacts)

    def sync_bodies(self, hub_name, bin:bytes):
        inArray = loads(bin)
        _bodies = []
        for v_b9d4d75c_94f9_5d5f_95df_55feb77f8710 in inArray[0]:
            _bodies.append(body_snapshot_to_protcol(v_b9d4d75c_94f9_5d5f_95df_55feb77f8710))
        s = session(hub_name)
        for fn in self.on_sync_bodies:
            fn(s, _bodies)

    def sync_contacts(self, hub_name, bin:bytes):
        inArray = loads(bin)
        _contacts = []
        for v_717cc1ef_9524_5a4a_84a6_53af7ecc3682 in inArray[0]:
            _contacts.append(contact_info_to_protcol(v_717cc1ef_9524_5a4a_84a6_53af7ecc3682))
        s = session(hub_name)
        for fn in self.on_sync_contacts:
            fn(s, _contacts)




