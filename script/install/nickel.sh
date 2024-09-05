#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
NICKEL_ARCH="$ARCH"
NICKEL_VERSION="1.7.0"

if [[ -f "${VORPAL_PATH_ENV_BIN}/nickel" ]]; then
    "${VORPAL_PATH_ENV_BIN}/nickel" --version
    exit 0
fi

if [ "$ARCH" = "aarch64" ]; then
    NICKEL_ARCH="arm64";
fi

if [ "$OS" == "darwin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"

    cargo install --root "${VORPAL_PATH_ENV}" nickel-lang-cli
fi

if [ "$OS" == "linux" ]; then
    curl -L \
        "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
        -o "${VORPAL_PATH_ENV_BIN}/nickel"

    chmod +x "${VORPAL_PATH_ENV_BIN}/nickel"
fi
