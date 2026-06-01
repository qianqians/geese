# -*- coding: UTF-8 -*-
from .msgpack import *

class RankItem(object):
    def __init__(self, key:str, score:float, profile:bytes):
        self.key = key
        self.score = score
        self.profile = profile
        
    def info(self):
        return dumps({"key": self.key, "score": self.score, "profile": self.profile})
        
def __create_rank_key__(rankName:str):
    return "rank:{}".format(rankName)
        
class Rank(object):
    def __init__(self, rankName:str):
        self.rankName = __create_rank_key__(rankName)
        
    def __add_rank__(self, item:RankItem):
        from .app import app
        app().redis_proxy.set(item.key, item.info())
        app().redis_proxy.zadd(self.rankName, {item.key: item.score})
        
    def del_rank(self, itemKey:list[str]):
        from .app import app
        app().redis_proxy.zrem(self.rankName, *itemKey)
        app().redis_proxy.delete(*itemKey)
        
    def update_rank(self, item:RankItem):
        self.del_rank([item.key])
        self.__add_rank__(item)
        
    def get_rank(self, itemKey:str) -> tuple[int, RankItem]:
        from .app import app
        rank = app().redis_proxy.zrevrank(self.rankName, itemKey)
        info = loads(app().redis_proxy.get(itemKey))
        return (rank, RankItem(info["key"], info["score"], info["profile"]))
    
    def get_rank_list(self, start:int, count:int) -> list[RankItem]:
        from .app import app
        keys = app().redis_proxy.zrevrange(self.rankName, start, start + count)
        rankItemList = []
        for k in keys:
            info = loads(app().redis_proxy.get(k))
            rankItemList.append(RankItem(info["key"], info["score"], info["profile"]))
        return rankItemList