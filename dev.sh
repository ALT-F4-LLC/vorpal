#!/usr/bin/env bash
set -euo pipefail

if [ "${VORPAL_DEV:-}" != "$PWD" ]; then
    export VORPAL_DEV="$PWD"

    docker buildx build \
        --build-arg "GROUP_ID=$(id -g)" \
        --build-arg "USER_ID=$(id -u)" \
        --tag "ghcr.io/alt-f4-llc/vorpal:edge-dev" \
        --target "dev" \
        .

    export PATH=$PWD/.vorpal/bin:$PATH

    if [ $# -gt 0 ]; then
        exec "$@"
    else
        exec "$SHELL"
    fi
fi
