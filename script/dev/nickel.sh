#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
NICKEL_ARCH="$ARCH"
NICKEL_VERSION="1.7.0"
OS="$(uname | tr '[:upper:]' '[:lower:]')"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

if [[ -f "${1}/bin/nickel" ]]; then
    "${1}/bin/nickel" --version
    exit 0
fi

if [ "$ARCH" = "aarch64" ]; then
    NICKEL_ARCH="arm64";
fi

if [ "$OS" == "darwin" ]; then
    PATH="$HOME/.cargo/bin:$PATH"

    cargo install --root "${1}" nickel-lang-cli
fi

if [ "$OS" == "linux" ]; then
    curl -L \
        "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
        -o "${1}/bin/nickel"

    chmod +x "${1}/bin/nickel"
fi
