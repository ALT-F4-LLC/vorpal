#!/usr/bin/env bash
set -euxo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <rustup_confirm>"
    exit 1
fi

if $RUSTUP_CONFIRM; then
    confirm="y"
else
    read -r -p "Do you want to install rustup? (y/n): " confirm
fi

if [[ "$confirm" == "y" ]]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- --default-toolchain 'none' --no-modify-path --profile 'minimal' -y
else
    echo "Installation aborted."
    exit 1
fi
