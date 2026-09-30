# -*- coding: UTF-8 -*-

class base_dbproxy_handle(object):
    def __init__(self):
        from .app import app
        self.__dbproxy__ = app().dbproxy_mgr.get_dbproxy()
        
    def __get_dbproxy__(self):
        return self.__dbproxy__