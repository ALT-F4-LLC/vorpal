#!/usr/bin/env bash
set -euo pipefail

# Environment variables
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
PATH="${PWD}/script/bin:${PWD}/.env/bin:${PATH}"
VORPAL_PATH="/var/lib/vorpal"

# Build variables
BINUTILS_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/binutils")"
BINUTILS_STORE_PATH="${VORPAL_PATH}/store/binutils-${BINUTILS_SOURCE_HASH}"
BINUTILS_STORE_PATH_PACKAGE="${BINUTILS_STORE_PATH}.package"
BINUTILS_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/binutils-${BINUTILS_SOURCE_HASH}"
BINUTILS_STORE_PATH_SOURCE="${BINUTILS_STORE_PATH}.source"
BINUTILS_VERSION="2.43.1"

if [ -d "${BINUTILS_STORE_PATH_PACKAGE}" ]; then
    echo "binutils-${BINUTILS_SOURCE_HASH}"
    exit 0
fi

if [ ! -d "${BINUTILS_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/binutils/binutils-${BINUTILS_VERSION}.tar.gz" \
        -o "/tmp/binutils-${BINUTILS_VERSION}.tar.gz"
    tar -xvzf "/tmp/binutils-${BINUTILS_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$(hash_path "/tmp/binutils-${BINUTILS_VERSION}")
    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$SOURCE_HASH" != "$BINUTILS_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: ${SOURCE_HASH} != ${BINUTILS_SOURCE_HASH}"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/binutils-${BINUTILS_VERSION}" . | zstd -o "${BINUTILS_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${BINUTILS_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${BINUTILS_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${BINUTILS_STORE_PATH_SOURCE}"

    rm -rf "/tmp/binutils-${BINUTILS_VERSION}"
    rm -rf "/tmp/binutils-${BINUTILS_VERSION}.tar.gz"
fi

mkdir -p "${BINUTILS_STORE_PATH_SANDBOX}"

cp -r "${BINUTILS_STORE_PATH_SOURCE}/." "${BINUTILS_STORE_PATH_SANDBOX}"

pushd "${BINUTILS_STORE_PATH_SANDBOX}"

./configure --prefix="${BINUTILS_STORE_PATH_PACKAGE}"
make
make install

popd

tar -cvf - -C "${BINUTILS_STORE_PATH_PACKAGE}" . | zstd -o "${BINUTILS_STORE_PATH_PACKAGE}.tar.zst"

rm -rf "${BINUTILS_STORE_PATH_SANDBOX}"
