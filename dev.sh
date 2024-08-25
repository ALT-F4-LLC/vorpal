#!/usr/bin/env bash
set -euo pipefail

docker buildx build \
    --build-arg "GROUP_ID=$(id -g)" \
    --build-arg "USER_ID=$(id -u)" \
    --tag "ghcr.io/alt-f4-llc/vorpal:edge-dev" \
    --target "dev" \
    .

source "${PWD}/.env"
