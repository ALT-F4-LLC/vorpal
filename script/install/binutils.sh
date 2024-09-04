#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <prefix_path>"
    exit 1
fi

echo "Install binutils -> $1"

BINUTILS_VERSION="2.43.1"
PREFIX_PATH="$1"

curl -L \
    "https://ftp.gnu.org/gnu/binutils/binutils-${BINUTILS_VERSION}.tar.gz" \
    -o "/tmp/binutils-${BINUTILS_VERSION}.tar.gz"

tar -xzf "/tmp/binutils-${BINUTILS_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/binutils-${BINUTILS_VERSION}"

./configure --prefix="${PREFIX_PATH}"

make -j"$(nproc)"

make install

popd

rm -rf "/tmp/binutils-${BINUTILS_VERSION}"

rm -rf "/tmp/binutils-${BINUTILS_VERSION}.tar.gz"
