#!/usr/bin/env bash
set -euo pipefail

# Environment variables
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
PATH="${PWD}/script/bin:${PWD}/.env/bin:${PATH}"
VORPAL_PATH="/var/lib/vorpal"

# Build variables
ZSTD_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/zstd")"
ZSTD_STORE_PATH="${VORPAL_PATH}/store/zstd-${ZSTD_SOURCE_HASH}"
ZSTD_STORE_PATH_PACKAGE="${ZSTD_STORE_PATH}.package"
ZSTD_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/zstd-${ZSTD_SOURCE_HASH}"
ZSTD_STORE_PATH_SOURCE="${ZSTD_STORE_PATH}.source"
ZSTD_VERSION="1.5.5"

# Environment variables
PATH="${ZSTD_STORE_PATH_PACKAGE}/bin:${PATH}"

compile_source() {
    pushd "${ZSTD_STORE_PATH_SANDBOX}"

    make
    make install PREFIX="${ZSTD_STORE_PATH_PACKAGE}"

    popd

    rm -rf "${ZSTD_STORE_PATH_SANDBOX}"
}

if [ -d "${ZSTD_STORE_PATH_PACKAGE}" ]; then
    echo "zstd-${ZSTD_SOURCE_HASH}"
    exit 0
fi

if [ ! -d "${ZSTD_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://github.com/facebook/zstd/releases/download/v${ZSTD_VERSION}/zstd-${ZSTD_VERSION}.tar.gz" \
        -o "/tmp/zstd-${ZSTD_VERSION}.tar.gz"
    tar -xzf "/tmp/zstd-${ZSTD_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$(hash_path "/tmp/zstd-${ZSTD_VERSION}")
    echo "Calculated source hash: $SOURCE_HASH"

    if [ "$SOURCE_HASH" != "$ZSTD_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: $SOURCE_HASH != $ZSTD_SOURCE_HASH"
        exit 1
    fi

    mkdir -p "${ZSTD_STORE_PATH_SANDBOX}"

    cp -r "/tmp/zstd-${ZSTD_VERSION}/." "${ZSTD_STORE_PATH_SANDBOX}"

    compile_source

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/zstd-${ZSTD_VERSION}" . | zstd -o "${ZSTD_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${ZSTD_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${ZSTD_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${ZSTD_STORE_PATH_SOURCE}"

    rm -rf "${ZSTD_STORE_PATH_PACKAGE}"
    rm -rf "/tmp/zstd-${ZSTD_VERSION}"
    rm -rf "/tmp/zstd-${ZSTD_VERSION}.tar.gz"
fi

mkdir -p "${ZSTD_STORE_PATH_SANDBOX}"

cp -r "${ZSTD_STORE_PATH_SOURCE}/." "${ZSTD_STORE_PATH_SANDBOX}"

compile_source

tar -cvf - -C "${ZSTD_STORE_PATH_PACKAGE}" . | zstd -o "${ZSTD_STORE_PATH_PACKAGE}.tar.zst"
