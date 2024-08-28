#!/usr/bin/env bash
set -euo pipefail

if [ -z "${1-}" ]; then
    echo "Usage: $0 {clean|start|stop}"
    exit 1
fi

if [ "$1" == "clean" ]; then
    docker image rm --force "ghcr.io/alt-f4-llc/vorpal:edge-dev"
    docker image rm --force "localhost:5000/vorpal:edge-dev"
    rm -rf .vorpal/registry
    rm -rf .vorpal/registry-id
fi

if [ ! -f .vorpal/registry-id ]; then
    echo "$RANDOM" > .vorpal/registry-id
fi

if [ "$1" == "start" ]; then
    CONTAINER_NAME="vorpal-registry-$(cat .vorpal/registry-id)"

    if [ ! "$(docker ps -q -f name="$CONTAINER_NAME")" ]; then
        REGISTRY_HOST="127.0.0.1:5000"
        REGISTRY_ID=$(docker container run \
            --detach \
            --name "$CONTAINER_NAME" \
            --publish "$REGISTRY_HOST:5000" \
            --quiet \
            --rm \
            --volume "$(pwd)/.vorpal/registry:/var/lib/registry" \
            registry:2)
        echo "Registry: $CONTAINER_NAME:$REGISTRY_ID [$REGISTRY_HOST:5000]"
    fi

    IMAGE_DIGEST=$(docker buildx build \
        --cache-from "type=registry,ref=localhost:5000/vorpal:edge-dev-cache" \
        --cache-to "type=registry,ref=localhost:5000/vorpal:edge-dev-cache" \
        --push \
        --quiet \
        --tag "localhost:5000/vorpal:edge-dev" \
        --target "dev" \
        .)

    echo "Digest: $IMAGE_DIGEST"

    docker image tag \
        "localhost:5000/vorpal:edge-dev" \
        "ghcr.io/alt-f4-llc/vorpal:edge-dev"

    if [ $# -gt 1 ]; then
        export NICKEL_IMPORT_PATH="${PWD}/.vorpal/packages:${PWD}"
        export PATH="${PWD}/.vorpal/bin:$PATH"
        shift
        exec "$@"
    fi

    exit 0
fi

if [ "$1" == "stop" ]; then
    docker container rm --force "vorpal-registry-$(cat .vorpal/registry-id)" || true
    rm -rf .vorpal/registry-id
    exit 0
fi
