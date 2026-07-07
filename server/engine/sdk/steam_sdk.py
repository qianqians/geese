# -*- coding: UTF-8 -*-
import sys
import aiohttp
import asyncio
import json

SteamApiUrl = "https://partner.steamgames.com/ISteamUserAuth/AuthenticateUserTicket/v1/"

async def code2Session(appid:str, secret:str, ticket:str) -> dict:
    data = {
        'key': secret,
        'appid': appid,
        'ticket': ticket
    }
    async with aiohttp.ClientSession() as session:
        async with session.post(SteamApiUrl, json=data) as response:
            result = await response.json()
            if result != None:
                return result
    return None