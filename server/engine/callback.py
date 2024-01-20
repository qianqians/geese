# -*- coding: UTF-8 -*-
from collections.abc import Callable
from threading import Timer

class callback(object):
    def __init__(self, release_handle:Callable[[], bool]):
        self._callback:Callable[[bytes],None] = None
        self.error:Callable[[bytes],None] = None
        self._timeout:Callable[[],None] = None

        self.release_handle = release_handle

    def callback(self, rsp_callback:Callable[[bytes],None], err_callback:Callable[[bytes],None]):
        self._callback = rsp_callback
        self.error = err_callback

    def __call_timeout__(self):
        if self.release_handle():
            self._timeout()

    def timeout(self, _timeout:int, time_callback:Callable[[],None]):
        self._timeout = time_callback
        Timer(_timeout/1000, self.__call_timeout__).start()