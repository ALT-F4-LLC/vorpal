#!/bin/bash
set -eo pipefail

if [ -t 1 ]; then
    TTY_OPTS="--tty"
else
    TTY_OPTS=""
fi

SCRIPT_NAME=$(basename "$0")

docker container run ${TTY_OPTS} \
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
