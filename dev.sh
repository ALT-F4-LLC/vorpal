#!/usr/bin/env bash
set -euo pipefail

docker buildx build \
    --build-arg "GROUP_ID=$(id -g)" \
    --build-arg "USER_ID=$(id -u)" \
    --tag "ghcr.io/alt-f4-llc/vorpal:edge-dev" \
    --target "dev" \
    .

if [ $# -ne 0 ]; then
    export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
    export PATH="${PWD}/.vorpal/bin:$PATH"
    exec "$@"
fi
