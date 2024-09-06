#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
NICKEL_ARCH="$ARCH"
NICKEL_VERSION="1.7.0"
OS="$(uname | tr '[:upper:]' '[:lower:]')"

if [[ -f "${ENV_PATH}/bin/nickel" ]]; then
    "${ENV_PATH}/bin/nickel" --version
    exit 0
fi

if [ "$ARCH" = "aarch64" ]; then
    NICKEL_ARCH="arm64";
fi

if [ "$OS" == "darwin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"

    cargo install --root "${ENV_PATH}" nickel-lang-cli
fi

if [ "$OS" == "linux" ]; then
    curl -L \
        "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
        -o "${ENV_PATH}/bin/nickel"

    chmod +x "${ENV_PATH}/bin/nickel"
fi
