cd ../../tools/thrift/windows
thrift -out ../../../crates/proto/src --gen rs ../../../crates/proto/proto/common.thrift
thrift -out ../../../crates/proto/src --gen rs ../../../crates/proto/proto/client.thrift
thrift -out ../../../crates/proto/src --gen rs ../../../crates/proto/proto/gate.thrift
thrift -out ../../../crates/proto/src --gen rs ../../../crates/proto/proto/hub.thrift
thrift -out ../../../crates/proto/src --gen rs ../../../crates/proto/proto/dbproxy.thrift

cd ../../../crates/proto/
thrift-typescript --target apache --sourceDir ./proto --outDir ../../expand/ts/engine/proto common.thrift client.thrift gate.thrift

pause