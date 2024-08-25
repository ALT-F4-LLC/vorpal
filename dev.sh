#!/usr/bin/env bash
set -euo pipefail

export PATH=$PWD/.vorpal/bin:$PATH

docker buildx build \
    --build-arg "GROUP_ID=$(id -g)" \
    --build-arg "USER_ID=$(id -u)" \
    --tag "ghcr.io/alt-f4-llc/vorpal:edge-dev" \
    --target "dev" \
    .

if [[ "$PATH" != *"$PWD/.vorpal/bin"* ]]; then
    exec "$SHELL"
else
    exec "$@"
fi
