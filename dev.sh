#!/usr/bin/env bash
set -euo pipefail

if [ -z "${BUILDX_CACHE_FROM:-}" ]; then
    BUILDX_CACHE_FROM="type=registry,ref=ghcr.io/alt-f4-llc/vorpal-dev:edge-cache"
fi

echo "Cache from: ${BUILDX_CACHE_FROM}"

docker buildx build \
    --cache-from "${BUILDX_CACHE_FROM}" \
    --file "Dockerfile.dev" \
    --load \
    --tag "ghcr.io/alt-f4-llc/vorpal-dev:edge" \
    .

if [ $# -gt 1 ]; then
    export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
    export PATH="${PWD}/.vorpal/bin:$PATH"
    exec "$@"
fi
