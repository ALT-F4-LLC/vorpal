#!/usr/bin/env bash
set -euo pipefail

VERSION="14.2.0"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

# Build variables

curl -L \
    "https://ftp.gnu.org/gnu/gcc/gcc-${VERSION}/gcc-${VERSION}.tar.gz" \
    -o "/tmp/gcc-${VERSION}.tar.gz"

tar -xzf "/tmp/gcc-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/gcc-${VERSION}"

./contrib/download_prerequisites

mkdir -p ./build

popd

pushd "/tmp/gcc-${VERSION}/build"

../configure \
    --disable-multilib \
    --enable-languages="c,c++" \
    --prefix="${1}"

make -j$(nproc)

make install

popd

rm -rf "/tmp/gcc-${VERSION}"

rm -rf "/tmp/gcc-${VERSION}.tar.gz"
