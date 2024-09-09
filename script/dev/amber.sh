#!/usr/bin/env bash
set -euo pipefail

AMBER_SYSTEM=""
AMBER_VERSION="0.3.5-alpha"
ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname | tr '[:upper:]' '[:lower:]')

if [[ -f "${ENV_PATH}/bin/amber" ]]; then
    "${ENV_PATH}/bin/amber" --version | head -n 1
    exit 0
fi

if [[ "${OS}" == "darwin" ]]; then
    AMBER_SYSTEM="apple-darwin"
elif [[ "${OS}" == "linux" ]]; then
    AMBER_SYSTEM="unknown-linux-gnu"
else
    echo "Unsupported OS: ${OS}"
    exit 1
fi

if [[ "${ARCH}" == "x86_64" ]]; then
    ARCH="x86_64"
elif [[ "${ARCH}" == "arm64" || "${ARCH}" == "aarch64" ]]; then
    ARCH="aarch64"
else
    echo "Unsupported ARCH: ${ARCH}"
    exit 1
fi

curl -L \
    "https://github.com/amber-lang/amber/releases/download/${AMBER_VERSION}/amber-${ARCH}-${AMBER_SYSTEM}.tar.xz" \
    -o "/tmp/amber-${AMBER_VERSION}.tar.xz"

tar -xf "/tmp/amber-${AMBER_VERSION}.tar.xz" -C "/tmp"

if [[ "${OS}" == "darwin" ]]; then
    cp "/tmp/amber-${ARCH}-${AMBER_SYSTEM}/amber" "${ENV_PATH}/bin/amber"
else
    ls -alh /tmp
    cp "/tmp/amber" "${ENV_PATH}/bin/amber"
fi

rm -rf "/tmp/amber-${AMBER_VERSION}.tar.gz"
