from threading import Timer
from collections.abc import Callable
from enum import *
from .engine import *
from .engine.msgpack import *
from .common_svr import *

# this enum code is codegen by geese codegen for python

#this struct code is codegen by geese codegen for python
#this caller code is codegen by geese codegen for python
class player_ntf_client_caller(object):
    def __init__(self, entity:player):
        self.entity = entity

    def drop(self, task_id:int, pkg_item:list[item], total_item:list[item]):
        _argv_eb03ad59_660a_30d5_8232_5fb4f9c07d3f = []
        _argv_eb03ad59_660a_30d5_8232_5fb4f9c07d3f.append(task_id)
        _list_41373670_f5d5_318d_b7f7_c3a4f5036023 = []
        for v_5441b1b9_4592_5e1c_94ba_124d9c6a6f94 in pkg_item:
            _list_41373670_f5d5_318d_b7f7_c3a4f5036023.append(item_to_protcol(v_5441b1b9_4592_5e1c_94ba_124d9c6a6f94))
        _argv_eb03ad59_660a_30d5_8232_5fb4f9c07d3f.append(_list_41373670_f5d5_318d_b7f7_c3a4f5036023)
        _list_08bdea7a_087c_3a03_a2ce_2b98ccf14532 = []
        for v_e22d0078_3b03_5fe2_a310_c3f6c3c23740 in total_item:
            _list_08bdea7a_087c_3a03_a2ce_2b98ccf14532.append(item_to_protcol(v_e22d0078_3b03_5fe2_a310_c3f6c3c23740))
        _argv_eb03ad59_660a_30d5_8232_5fb4f9c07d3f.append(_list_08bdea7a_087c_3a03_a2ce_2b98ccf14532)
        self.entity.call_client_main_notify("drop", dumps(_argv_eb03ad59_660a_30d5_8232_5fb4f9c07d3f))

    def task(self, task_list:list[task_info], progress_list:list[task_progress_info]):
        _argv_4dd64667_0e3f_3140_8e0f_c10e48418c0a = []
        _list_5d481c82_620e_3d22_a61d_abbbefd01ca3 = []
        for v_944f018b_64ee_5e22_a445_39fa78b68397 in task_list:
            _list_5d481c82_620e_3d22_a61d_abbbefd01ca3.append(task_info_to_protcol(v_944f018b_64ee_5e22_a445_39fa78b68397))
        _argv_4dd64667_0e3f_3140_8e0f_c10e48418c0a.append(_list_5d481c82_620e_3d22_a61d_abbbefd01ca3)
        _list_d4c05ccf_6ec1_3b5f_bf69_588ab838173f = []
        for v_553ebab7_911a_59be_9a8f_20354c69a863 in progress_list:
            _list_d4c05ccf_6ec1_3b5f_bf69_588ab838173f.append(task_progress_info_to_protcol(v_553ebab7_911a_59be_9a8f_20354c69a863))
        _argv_4dd64667_0e3f_3140_8e0f_c10e48418c0a.append(_list_d4c05ccf_6ec1_3b5f_bf69_588ab838173f)
        self.entity.call_client_main_notify("task", dumps(_argv_4dd64667_0e3f_3140_8e0f_c10e48418c0a))



