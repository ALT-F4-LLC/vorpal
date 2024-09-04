#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <prefix_path>"
    exit 1
fi

echo "Install gcc -> $1"

GCC_VERSION="14.2.0"
PREFIX_PATH="$1"
# PATH="${PREFIX_PATH}/bin:${PATH}"

curl -L \
    "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.gz" \
    -o "/tmp/gcc-${GCC_VERSION}.tar.gz"

tar -xzf "/tmp/gcc-${GCC_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/gcc-${GCC_VERSION}"

./contrib/download_prerequisites

mkdir -p build

popd

pushd "/tmp/gcc-${GCC_VERSION}/build"

../configure --enable-languages="c,c++,go" --prefix="${PREFIX_PATH}"

make -j"$(nproc)"

make install

popd

rm -rf "/tmp/gcc-${GCC_VERSION}/build"

rm -rf "/tmp/gcc-${GCC_VERSION}"

rm -f "/tmp/gcc-${GCC_VERSION}.tar.gz"
