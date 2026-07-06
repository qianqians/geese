cd /d ../../server/dependences/consul/
start consul.exe agent -dev

cd /d ../redis/
start start.bat

timeout /t 3

cd /d ../../../sample/server/bin
start dbproxy.exe ../config/dbproxy.cfg
start gate.exe ../config/gate.cfg

timeout /t 3

cd /d ../src
start python rank_app.py ../config/rank.cfg

timeout /t 3

start python app.py ../config/player.cfg

cd ../
pause