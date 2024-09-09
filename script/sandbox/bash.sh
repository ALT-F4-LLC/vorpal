#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
CPU_COUNT=""
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VERSION="5.2"

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
    "https://ftp.gnu.org/gnu/bash/bash-${VERSION}.tar.gz" \
    -o "/tmp/bash-${VERSION}.tar.gz"

tar -xvzf "/tmp/bash-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/bash-${VERSION}"

./configure --prefix="${1}"

make ${CPU_COUNT}

make install

popd

rm -rf "/tmp/bash-${VERSION}"

rm -rf "/tmp/bash-${VERSION}.tar.gz"
