# -*- coding: UTF-8 -*-

class base_entity(object):
    def __init__(self, entity_type:str, entity_id:str) -> None:
        self.entity_type = entity_type
        self.entity_id = entity_id

    def trace(self, format:str, *argv):
        from app import app
        app().ctx.log("trace", "{} entity:{}, ".format(self.entity_type, self.entity_id) + format.format(argv))
        
    def debug(self, format:str, *argv):
        from app import app
        app().ctx.log("debug", "{} entity:{}, ".format(self.entity_type, self.entity_id) + format.format(argv))

    def info(self, format:str, *argv):
        from app import app
        app().ctx.log("info", "{} entity:{}, ".format(self.entity_type, self.entity_id) + format.format(argv))

    def warn(self, format:str, *argv):
        from app import app
        app().ctx.log("warn", "{} entity:{}, ".format(self.entity_type, self.entity_id) + format.format(argv))

    def error(self, format:str, *argv):
        from app import app
        app().ctx.log("error", "{} entity:{}, ".format(self.entity_type, self.entity_id) + format.format(argv))