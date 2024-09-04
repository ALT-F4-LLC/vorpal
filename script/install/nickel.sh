#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "Usage: $0 <sandbox_path> <arch> <os>"
    exit 1
fi

ARCH="$2"
NICKEL_ARCH="$ARCH"
NICKEL_VERSION="1.7.0"
OS="$3"
SANDBOX_PATH="$1"

if [ "$ARCH" = "aarch64" ]; then
    NICKEL_ARCH="arm64";
fi

if [ "$OS" == "darwin" ]; then
    cargo install nickel-lang-cli

    cp "$(which nickel)" "${SANDBOX_PATH}/bin/nickel"
fi

if [ "$OS" == "linux" ]; then
    curl -L \
        "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
        -o "${SANDBOX_PATH}/bin/nickel"

    chmod +x "${SANDBOX_PATH}/bin/nickel"
fi
