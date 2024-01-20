# -*- coding: UTF-8 -*-

class base_entity(object):
    def __init__(self, entity_type:str, entity_id:str) -> None:
        self.entity_type = entity_type
        self.entity_id = entity_id