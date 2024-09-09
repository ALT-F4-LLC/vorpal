#!/usr/bin/env bash
set -euo pipefail

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

SANDBOX_PACKAGE_PATH="$1"

# Environment variables
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VORPAL_PATH="/var/lib/vorpal"

# Build variables
SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/gcc")"
STORE_PATH="${VORPAL_PATH}/store/gcc-${SOURCE_HASH}"
STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/gcc-${SOURCE_HASH}"
STORE_PATH_SOURCE="${STORE_PATH}.source"
VERSION="14.2.0"

if [ ! -d "${STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/gcc/gcc-${VERSION}/gcc-${VERSION}.tar.gz" \
        -o "/tmp/gcc-${VERSION}.tar.gz"
    tar -xzf "/tmp/gcc-${VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    DOWNLOAD_SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/gcc-${VERSION}")
    echo "Calculated source hash: $SOURCE_HASH"

    if [ "$DOWNLOAD_SOURCE_HASH" != "$SOURCE_HASH" ]; then
        echo "Download hash mismatch: $DOWNLOAD_SOURCE_HASH != $SOURCE_HASH"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/gcc-${VERSION}" . | zstd -o "${STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${STORE_PATH_SOURCE}"

    rm -rf "/tmp/gcc-${VERSION}"
    rm -rf "/tmp/gcc-${VERSION}.tar.gz"
fi

mkdir -p "${STORE_PATH_SANDBOX}"

cp -r "${STORE_PATH_SOURCE}/." "${STORE_PATH_SANDBOX}"

pushd "${STORE_PATH_SANDBOX}"

./contrib/download_prerequisites

mkdir -p ./build

popd

pushd "${STORE_PATH_SANDBOX}/build"

../configure --enable-languages="c,c++" --prefix="${SANDBOX_PACKAGE_PATH}"
make -j$(nproc)
make install

popd

rm -rf "${STORE_PATH_SANDBOX}"
