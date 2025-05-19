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
    --exclude="dist" \
    --exclude="target" \
    "$PWD/." "$HOME/vorpal/."

    pushd "$HOME/vorpal"

    ./script/dev.sh make

    popd
}

function install {
    sync

    sudo rm -rf /var/lib/vorpal
    sudo mkdir -pv /var/lib/vorpal/{key,sandbox,store}
    sudo mkdir -pv /var/lib/vorpal/store/artifact/{alias,archive,config,output}
    sudo chown -R "$(id -u):$(id -g)" /var/lib/vorpal

    pushd "$HOME/vorpal"

    ./target/debug/vorpal keys generate

    popd
}

COMMAND="${1:-}"

if [[ -z "$COMMAND" ]]; then
    echo "Usage: $0 <command>"
    echo "Available commands: deps, install, sync"
    exit 1
fi

if [[ "$COMMAND" != "deps" && "$COMMAND" != "install" && "$COMMAND" != "sync" ]]; then
    echo "Invalid command: $COMMAND"
    echo "Available commands: deps, install, sync"
    exit 1
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
