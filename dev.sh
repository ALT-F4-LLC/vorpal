#!/usr/bin/env bash
set -euo pipefail

start_time=$(date +%s)

BUILD_DIGEST=$(docker buildx build \
    --cache-from "type=local,src=${PWD}/.vorpal/buildx" \
    --cache-to "type=local,dest=${PWD}/.vorpal/buildx,mode=max" \
    --file "Dockerfile.dev" \
    --quiet \
    --tag "ghcr.io/alt-f4-llc/vorpal-dev:edge" \
    .)

end_time=$(date +%s)

duration=$((end_time - start_time))

echo "Build digest: ${BUILD_DIGEST}"
echo "Build duration: ${duration}s"

TTY_OPTS=""
VOLUME_OPTS=""

if [ -t 1 ]; then
    TTY_OPTS="--tty"
fi

if [ -d /nix/store ]; then
    VOLUME_OPTS="--volume /nix/store:/nix/store"
fi

if [ $# -gt 1 ]; then
    docker container run ${TTY_OPTS} ${VOLUME_OPTS} \
        --env "NICKEL_IMPORT_PATH=${PWD}/.vorpal/packages:${PWD}" \
        --interactive \
        --network "host" \
        --rm \
        --volume "${HOME}/.cargo:/root/.cargo" \
        --volume "${PWD}:${PWD}" \
        --volume "/var/lib/vorpal:/var/lib/vorpal" \
        --volume "/var/run/docker.sock:/var/run/docker.sock" \
        --workdir "${PWD}" \
        ghcr.io/alt-f4-llc/vorpal-dev:edge \
        "$@"
fi
