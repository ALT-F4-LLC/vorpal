#!/bin/bash
set -eo pipefail

SCRIPT_NAME=$(basename "$0")
TTY_OPTS=""
VOLUME_OPTS=""

if [ -t 1 ]; then
    TTY_OPTS="--tty"
fi

if [ -d /nix/store ]; then
    VOLUME_OPTS="--volume /nix/store:/nix/store"
fi

docker container run ${TTY_OPTS} ${VOLUME_OPTS} \
    --env "NICKEL_IMPORT_PATH=${PWD}/.vorpal/packages:${PWD}" \
    --interactive \
    --rm \
    --volume "${HOME}/.cargo:/root/.cargo" \
    --volume "${PWD}:${PWD}" \
    --volume "/var/lib/vorpal:/var/lib/vorpal" \
    --volume "/var/run/docker.sock:/var/run/docker.sock" \
    --workdir "${PWD}" \
    ghcr.io/alt-f4-llc/vorpal-dev:edge \
    "${SCRIPT_NAME}" \
    "$@"
