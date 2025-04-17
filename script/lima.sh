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
    --exclude=".packer" \
    --exclude=".vagrant" \
    --exclude="dist" \
    --exclude="packer_debian_vmware_arm64.box" \
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
