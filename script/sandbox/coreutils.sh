#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
CPU_COUNT=""
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VERSION="9.5"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

if [[ "${ARCH}" == "arm64" ]]; then
    ARCH="aarch64"
fi

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

curl -L \
    "https://ftp.gnu.org/gnu/coreutils/coreutils-${VERSION}.tar.gz" \
    -o "/tmp/coreutils-${VERSION}.tar.gz"

tar -xzf "/tmp/coreutils-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/coreutils-${VERSION}"

./configure --prefix="${1}"

make ${CPU_COUNT}

make install

popd

rm -rf "/tmp/coreutils-${VERSION}"

rm -rf "/tmp/coreutils-${VERSION}.tar.gz"
