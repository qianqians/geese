# -*- coding: UTF-8 -*-
import json

class RankItem(object):
    def __init__(self, key:str, score:float, profile:bytes):
        self.key = key
        self.score = score
        self.profile = profile
        
    def info(self):
        return json.dumps({"key": self.key, "score": self.score, "profile": self.profile})
        
def __create_rank_key__(rankName:str):
    return "rank:{}".format(rankName)
        
class Rank(object):
    def __init__(self, rankName:str):
        self.rankName = __create_rank_key__(rankName)
        
    def __add_rank__(self, item:RankItem):
        from .app import app
        app().redis_proxy.set(item.key, item.info())
        app().redis_proxy.zadd(self.rankName, {item.key: item.score})
        
    def __del_rank__(self, itemKey:str):
        from .app import app
        app().redis_proxy.zrem(self.rankName, itemKey)
        app().redis_proxy.delete(itemKey)
        
    def update_rank(self, item:RankItem):
        self.__del_rank__(item.key)
        self.__add_rank__(item)
        
    def get_rank(self, itemKey:str) -> int:
        from .app import app
        return app().redis_proxy.zrevrank(self.rankName, itemKey)
    
    def get_rank_list(self, start:int, count:int) -> list[RankItem]:
        from .app import app
        keys = app().redis_proxy.zrevrange(self.rankName, start, start + count)
        rankItemList = []
        for k in keys:
            rankItemList.append(json.loads(app().redis_proxy.get(k)))
        return rankItemList