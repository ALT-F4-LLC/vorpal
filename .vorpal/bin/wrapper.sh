#!/bin/bash
set -eo pipefail

SCRIPT_NAME=$(basename "$0")

if [ -t 1 ]; then
    TTY_OPTS="--tty"
else
    TTY_OPTS=""
fi

if [ "${SCRIPT_NAME}" != "just" ]; then
    USER_OPTS="--user $(id -u):$(id -g)"
else
    USER_OPTS=""
fi

docker container run ${TTY_OPTS} ${USER_OPTS} \
    --env "NICKEL_IMPORT_PATH=${PWD}/.vorpal/packages:${PWD}" \
    --interactive \
    --rm \
    --volume "${HOME}/.cargo:/home/devuser/.cargo" \
    --volume "/var/run/docker.sock:/var/run/docker.sock" \
    --volume "${PWD}:${PWD}" \
    --workdir "${PWD}" \
    ghcr.io/alt-f4-llc/vorpal:edge-dev \
    "${SCRIPT_NAME}" \
    "$@"
