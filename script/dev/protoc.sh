#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
PROTOC_SYSTEM=""
PROTOC_VERSION="28.0"

if [[ -f "${1}/bin/protoc" ]]; then
    exit 0
fi

if [[ "${OS}" == "darwin" ]]; then
    PROTOC_SYSTEM="osx"
elif [[ "${OS}" == "linux" ]]; then
    PROTOC_SYSTEM="linux"
else
    echo "Unsupported OS: ${OS}"
    exit 1
fi

if [[ "${ARCH}" == "x86_64" ]]; then
    PROTOC_SYSTEM="${PROTOC_SYSTEM}-x86_64"
elif [[ "${ARCH}" == "arm64" || "${ARCH}" == "aarch64" ]]; then
    PROTOC_SYSTEM="${PROTOC_SYSTEM}-aarch_64"
else
    echo "Unsupported ARCH: ${ARCH}"
    exit 1
fi

if [[ "$PROTOC_SYSTEM" == "" ]]; then
    echo "PROTOC_SYSTEM is empty"
    exit 1
fi

curl -L \
    "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip" \
    -o "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"

unzip "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip" -d "${1}"

rm -f "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"
