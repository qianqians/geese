cd ../../rpc

python genc2h.py python ../sample/proto/proto/client_call_hub ../sample/proto/proto/common ../sample/client/py/engine ../sample/server/src/engine
python genc2h.py ts ../sample/proto/proto/client_call_hub ../sample/proto/proto/common ../sample/client/ts/engine

python genh2c.py python ../sample/proto/proto/hub_call_client ../sample/proto/proto/common ../sample/client/py/engine ../sample/server/src/engine
python genh2c.py ts ../sample/proto/proto/hub_call_client ../sample/proto/proto/common ../sample/client/ts/engine

python genh2h.py python ../sample/proto/proto/hub_call_hub ../sample/proto/proto/common ../sample/server/src/engine

cd ../sample/proto

pause