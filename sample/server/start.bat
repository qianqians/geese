cd ../../server/dependences/consul/
start consul.exe agent -dev

cd ../redis/
start start.bat

timeout /t 3

cd ../../../sample/server/bin
start dbproxy.exe ../config/dbproxy.cfg
start gate.exe ../config/gate.cfg

timeout /t 3

cd ../src
start python app.py ../config/player.cfg

cd ../
pause