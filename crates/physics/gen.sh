#!/bin/bash
# 物理模块 RPC 代码生成脚本
# 从 crates/physics/proto 读取 .juggle IDL，生成 Python 桩代码到 crates/physics/python/

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RPC_DIR="$SCRIPT_DIR/../../rpc"

PROTO_DIR="$SCRIPT_DIR/proto"
COMMON_DIR="$PROTO_DIR/common"
CLIENT_CALL_HUB_DIR="$PROTO_DIR/client_call_hub"
HUB_CALL_CLIENT_DIR="$PROTO_DIR/hub_call_client"

CLI_OUT="$SCRIPT_DIR/python/client"
SVR_OUT="$SCRIPT_DIR/python/server"

cd "$RPC_DIR"

echo "=== genc2h: entity_service (client_call_hub) ==="
python3 genc2h.py python "$CLIENT_CALL_HUB_DIR" "$COMMON_DIR" "$CLI_OUT" "$SVR_OUT"

echo "=== genh2c: mutil_service (hub_call_client) ==="
python3 genh2c.py python "$HUB_CALL_CLIENT_DIR" "$COMMON_DIR" "$CLI_OUT" "$SVR_OUT"

echo "Done. Generated files:"
ls -la "$CLI_OUT"/*.py
ls -la "$SVR_OUT"/*.py
