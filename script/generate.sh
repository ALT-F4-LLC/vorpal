#!/usr/bin/env bash
#
# generate — regenerate the Go, TypeScript and Python API bindings from the
# protobuf definitions in sdk/rust/api, then run the Rust SDK codegen.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

protos=(
  agent/agent.proto
  artifact/artifact.proto
  archive/archive.proto
  context/context.proto
  worker/worker.proto
)

rm -rf sdk/go/pkg/api
mkdir -p sdk/go/pkg/api
for proto in "${protos[@]}"; do
  protoc \
    --go_opt=paths=source_relative \
    --go_out=sdk/go/pkg/api \
    --go-grpc_opt=paths=source_relative \
    --go-grpc_out=sdk/go/pkg/api \
    --proto_path=sdk/rust/api \
    "$proto"
done

rm -rf sdk/typescript/src/api
mkdir -p sdk/typescript/src/api
protoc \
  --plugin=protoc-gen-ts_proto=sdk/typescript/node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out=sdk/typescript/src/api \
  --ts_proto_opt=outputServices=grpc-js \
  --ts_proto_opt=esModuleInterop=true \
  --ts_proto_opt=snakeToCamel=false \
  --ts_proto_opt=forceLong=number \
  --ts_proto_opt=useOptionals=messages \
  --ts_proto_opt=oneof=unions \
  --ts_proto_opt=env=node \
  --ts_proto_opt=importSuffix=.js \
  --proto_path=sdk/rust/api \
  "${protos[@]}"

rm -rf sdk/python/src/vorpal_sdk/api
mkdir -p sdk/python/src/vorpal_sdk/api
uv run --project sdk/python --frozen --group dev python -m grpc_tools.protoc \
  --grpc_python_out=sdk/python/src/vorpal_sdk/api \
  --proto_path=sdk/rust/api \
  --pyi_out=sdk/python/src/vorpal_sdk/api \
  --python_out=sdk/python/src/vorpal_sdk/api \
  "${protos[@]}"
uv run --project sdk/python --frozen --group dev \
  python sdk/python/script/fix_proto_imports.py sdk/python/src/vorpal_sdk/api

cargo run -p vorpal-sdk-codegen
