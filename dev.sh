#!/usr/bin/env bash
set -euo pipefail

docker buildx build \
    --cache-from "type=local,src=${PWD}/.vorpal/buildx" \
    --cache-to "type=local,dest=${PWD}/.vorpal/buildx" \
    --load \
    --tag "ghcr.io/alt-f4-llc/vorpal:edge-dev" \
    --target "dev" \
    .

if [ $# -gt 1 ]; then
    export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
    export PATH="${PWD}/.vorpal/bin:$PATH"
    exec "$@"
fi
