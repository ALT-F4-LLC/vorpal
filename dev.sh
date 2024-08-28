#!/usr/bin/env bash
set -euo pipefail

docker buildx build \
    --cache-from "type=registry,ref=ghcr.io/alt-f4-llc/vorpal-dev:edge" \
    --file "Dockerfile.dev" \
    --progress "plain" \
    --tag "ghcr.io/alt-f4-llc/vorpal-dev:edge" \
    .

if [ $# -gt 1 ]; then
    export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
    export PATH="${PWD}/.vorpal/bin:$PATH"
    exec "$@"
fi
