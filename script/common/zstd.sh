#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
CPU_COUNT=""
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VERSION="1.5.5"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

if [[ -f "${1}/bin/zstd" ]]; then
    "${1}/bin/zstd" --version
    exit 0
fi

if [[ "${ARCH}" == "arm64" ]]; then
    ARCH="aarch64"
fi

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

curl -L \
    "https://github.com/facebook/zstd/releases/download/v${VERSION}/zstd-${VERSION}.tar.gz" \
    -o "/tmp/zstd-${VERSION}.tar.gz"

tar -xzf "/tmp/zstd-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/zstd-${VERSION}"

make ${CPU_COUNT}

make install PREFIX="${1}"

popd

rm -rf "/tmp/zstd-${VERSION}"

rm -rf "/tmp/zstd-${VERSION}.tar.gz"
