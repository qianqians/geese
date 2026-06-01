#coding:utf-8
# 2014-12-17
# build by qianqians
# deletenote

def deletenote(filestr:str):
    genfilestr:list[str] = []
    count:int = 0
    errornote:str = ""

    for i in range(len(filestr)):
        _str = filestr[i]

        while(1):
            if count == 1:
                indexafter = _str.find("*/")
                if indexafter != -1:
                    _str = _str[indexafter+2:]
                    count = 0
                else:
                    break

            index = _str.find('//')
            if index != -1:
                _str = _str[0:index]
            else:
                indexbegin = _str.find("/*")
                if indexbegin != -1:
                    errornote = _str
                    indexafter = _str.find("*/")
                    if indexafter != -1:
                        _str = _str[0:indexbegin] + _str[indexafter+2:]
                    else:
                        count = 1
                        break

            if _str != "":
                genfilestr.append(_str)

            break

    if count == 1:
        raise Exception("c/c++ coding error unpaired /* ", errornote)

    return genfilestr
