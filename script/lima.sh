#!/usr/bin/env bash
set -euo pipefail

function deps {
    "$PWD/script/dev/debian.sh"
}

function sync {
    deps

    mkdir -p "$HOME/vorpal"

    rsync -aPW \
    --delete \
    --exclude=".env" \
    --exclude=".git" \
    --exclude=".source-stage" \
    --exclude="dist" \
    --exclude="target" \
    --exclude="website" \
    "$PWD/." "$HOME/vorpal/."

    pushd "$HOME/vorpal"

    ./script/dev.sh make

    popd
}

function install {
    sync

    sudo rm -rf /var/lib/vorpal
    sudo mkdir -p /var/lib/vorpal/{key,sandbox,store}
    sudo mkdir -p /var/lib/vorpal/store/artifact/{alias,archive,config,output}
    sudo chown -R "$(id -u):$(id -g)" /var/lib/vorpal

    pushd "$HOME/vorpal"

    ./target/debug/vorpal system keys generate

    popd
}

function vorpal_build {
    cd "$HOME/vorpal"

    target/debug/vorpal build "$@"
}

function vorpal_start {
    exec "$HOME/vorpal/target/debug/vorpal" system services start "$@"
}

COMMAND="${1:-}"

if [[ -z "$COMMAND" ]]; then
    echo "Usage: $0 <command>"
    echo "Available commands: deps, install, sync, vorpal-build, vorpal-start"
    exit 1
fi

if [[ "$COMMAND" != "deps" && "$COMMAND" != "install" && "$COMMAND" != "sync" && "$COMMAND" != "vorpal-build" && "$COMMAND" != "vorpal-start" ]]; then
    echo "Invalid command: $COMMAND"
    echo "Available commands: deps, install, sync, vorpal-build, vorpal-start"
    exit 1
fi

if [[ "$COMMAND" == "vorpal-build" ]]; then
    shift
    vorpal_build "$@"
    exit 0
fi

if [[ "$COMMAND" == "vorpal-start" ]]; then
    shift
    vorpal_start "$@"
    exit 0
fi

if [[ "$COMMAND" == "deps" ]]; then
    deps
    exit 0
fi

if [[ "$COMMAND" == "install" ]]; then
    install
    exit 0
fi

if [[ "$COMMAND" == "sync" ]]; then
    sync
    exit 0
fi
