#!/usr/bin/env bash
set -euo pipefail

# Environment variables
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
PATH="${PWD}/script/bin:${PWD}/.env/bin:${PATH}"
VORPAL_PATH="/var/lib/vorpal"

# Build variables
GCC_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/gcc")"
GCC_STORE_PATH="${VORPAL_PATH}/store/gcc-${GCC_SOURCE_HASH}"
GCC_STORE_PATH_PACKAGE="${GCC_STORE_PATH}.package"
GCC_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/gcc-${GCC_SOURCE_HASH}"
GCC_STORE_PATH_SOURCE="${GCC_STORE_PATH}.source"
GCC_VERSION="14.2.0"

if [ -d "${GCC_STORE_PATH_PACKAGE}" ]; then
    echo "gcc already exists"
    exit 0
fi

if [ ! -d "${GCC_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.gz" \
        -o "/tmp/gcc-${GCC_VERSION}.tar.gz"
    tar -xzf "/tmp/gcc-${GCC_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$(hash_path "/tmp/gcc-${GCC_VERSION}")
    echo "Calculated source hash: $SOURCE_HASH"

    if [ "$SOURCE_HASH" != "$GCC_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: $SOURCE_HASH != $GCC_SOURCE_HASH"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/gcc-${GCC_VERSION}" . | zstd -o "${GCC_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${GCC_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${GCC_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${GCC_STORE_PATH_SOURCE}"

    rm -rf "/tmp/gcc-${GCC_VERSION}"
    rm -rf "/tmp/gcc-${GCC_VERSION}.tar.gz"
fi

mkdir -p "${GCC_STORE_PATH_SANDBOX}"

cp -r "${GCC_STORE_PATH_SOURCE}/." "${GCC_STORE_PATH_SANDBOX}"

pushd "${GCC_STORE_PATH_SANDBOX}"

./contrib/download_prerequisites

mkdir -p ./build

popd

pushd "${GCC_STORE_PATH_SANDBOX}/build"

../configure --enable-languages="c,c++" --prefix="${GCC_STORE_PATH_PACKAGE}"
make -j"$(nproc)"
make install

popd

tar -cvf - -C "${GCC_STORE_PATH_PACKAGE}" . | zstd -o "${GCC_STORE_PATH_PACKAGE}.tar.zst"

rm -rf "${GCC_STORE_PATH_SANDBOX}"
