#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d "$HOME/.rustup" ]]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- --default-toolchain 'none' --no-modify-path --profile 'minimal' -y
fi

"$HOME/.cargo/bin/cargo" --version
