#!/usr/bin/env bash
# Cross-platform proto generation script.
# Requires: thrift compiler (brew install thrift / apt install thrift-compiler)
#           thrift-typescript (npm install -g @creditkarma/thrift-typescript)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROTO_DIR="$SCRIPT_DIR/proto"
SRC_DIR="$SCRIPT_DIR/src"
TS_OUT_DIR="$SCRIPT_DIR/../../expand/ts/engine/proto"

# Detect thrift binary location
if command -v thrift &> /dev/null; then
    THRIFT="thrift"
elif [ -f "$SCRIPT_DIR/../../tools/thrift/windows/thrift.exe" ]; then
    echo "ERROR: Windows thrift binary found but running on Unix. Please install thrift: brew install thrift (macOS) or apt install thrift-compiler (Linux)"
    exit 1
else
    echo "ERROR: thrift compiler not found. Install it with: brew install thrift (macOS) or apt install thrift-compiler (Linux)"
    exit 1
fi

echo "Generating Rust code from Thrift definitions..."
for f in common client gate hub dbproxy; do
    echo "  $f.thrift -> Rust"
    "$THRIFT" -out "$SRC_DIR" --gen rs "$PROTO_DIR/$f.thrift"
done

echo "Generating TypeScript code from Thrift definitions..."
if command -v thrift-typescript &> /dev/null; then
    thrift-typescript --target apache --sourceDir "$PROTO_DIR" --outDir "$TS_OUT_DIR" common.thrift client.thrift gate.thrift
else
    echo "  WARNING: thrift-typescript not found. Skipping TypeScript generation."
    echo "  Install with: npm install -g @creditkarma/thrift-typescript"
fi

echo "Done."
