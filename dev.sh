#!/usr/bin/env bash
set -euo pipefail

docker buildx build \
    --cache-from "type=local,src=${PWD}/.vorpal/buildx" \
    --cache-from "type=registry,ref=ghcr.io/alt-f4-llc/vorpal-dev:edge-cache" \
    --cache-to "type=local,dest=${PWD}/.vorpal/buildx,compression=zstd,compression-level=22,image-manifest=true,oci-mediatypes=true,mode=max" \
    --file "Dockerfile.dev" \
    --load \
    --tag "ghcr.io/alt-f4-llc/vorpal-dev:edge" \
    .

if [ $# -gt 1 ]; then
    export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
    export PATH="${PWD}/.vorpal/bin:$PATH"
    exec "$@"
fi
