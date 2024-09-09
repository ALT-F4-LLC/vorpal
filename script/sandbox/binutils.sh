#!/usr/bin/env bash
set -euxo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
CPU_COUNT=""
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VERSION="2.43.1"

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
    "https://ftp.gnu.org/gnu/binutils/binutils-${VERSION}.tar.gz" \
    -o "/tmp/binutils-${VERSION}.tar.gz"

tar -xvzf "/tmp/binutils-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/binutils-${VERSION}"

mkdir -p ./build

popd

pushd "/tmp/binutils-${VERSION}/build"

../configure --prefix="${1}"
make ${CPU_COUNT}
make install

popd

rm -rf "/tmp/binutils-${VERSION}"

rm -rf "/tmp/binutils-${VERSION}.tar.gz"
