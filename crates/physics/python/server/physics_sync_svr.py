from threading import Timer
from collections.abc import Callable
from enum import Enum
from .engine import *
from .engine.msgpack import *
from .physics_common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class physics_sync_caller(object):
    def __init__(self, entity:player|entity):
        self.entity = entity

    def sync_bodies(self, bodies:list[body_snapshot]):
        _argv_dce5517c_26b4_3632_b0a1_387def836624 = []
        _list_c872cc08_25b2_36b1_9127_c123e8030361 = []
        for v_b9d4d75c_94f9_5d5f_95df_55feb77f8710 in bodies:
            _list_c872cc08_25b2_36b1_9127_c123e8030361.append(body_snapshot_to_protcol(v_b9d4d75c_94f9_5d5f_95df_55feb77f8710))
        _argv_dce5517c_26b4_3632_b0a1_387def836624.append(_list_c872cc08_25b2_36b1_9127_c123e8030361)
        self.entity.call_client_mutilcast("sync_bodies", dumps(_argv_dce5517c_26b4_3632_b0a1_387def836624))

    def sync_contacts(self, contacts:list[contact_info]):
        _argv_2d6479f7_5efd_3990_b470_06844d24719d = []
        _list_a55415c2_d4e0_3b4c_b8e9_5b7b98b3107e = []
        for v_717cc1ef_9524_5a4a_84a6_53af7ecc3682 in contacts:
            _list_a55415c2_d4e0_3b4c_b8e9_5b7b98b3107e.append(contact_info_to_protcol(v_717cc1ef_9524_5a4a_84a6_53af7ecc3682))
        _argv_2d6479f7_5efd_3990_b470_06844d24719d.append(_list_a55415c2_d4e0_3b4c_b8e9_5b7b98b3107e)
        self.entity.call_client_mutilcast("sync_contacts", dumps(_argv_2d6479f7_5efd_3990_b470_06844d24719d))



