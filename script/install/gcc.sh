#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <sandbox_path>"
    exit 1
fi

echo "Install gcc -> $1"

GCC_VERSION="14.2.0"
SANDBOX_PATH="$1"
PATH="${SANDBOX_PATH}/bin:${PATH}"

curl -L \
    "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.gz" \
    -o "/tmp/gcc-${GCC_VERSION}.tar.gz"

tar -xzf "/tmp/gcc-${GCC_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/gcc-${GCC_VERSION}"

./contrib/download_prerequisites

mkdir -p build

popd

pushd "/tmp/gcc-${GCC_VERSION}/build"

../configure --enable-languages="c,c++,go" --prefix="${SANDBOX_PATH}"

make

make install

popd

rm -rf "/tmp/gcc-${GCC_VERSION}/build"

rm -rf "/tmp/gcc-${GCC_VERSION}"

rm -f "/tmp/gcc-${GCC_VERSION}.tar.gz"
