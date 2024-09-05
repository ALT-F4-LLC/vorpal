#!/usr/bin/env bash
set -euo pipefail

COREUTILS_SOURCE_HASH="$(cat "${SCRIPT_PATH_INSTALL}/coreutils.sha256sum")"
COREUTILS_STORE_PATH="${VORPAL_PATH_STORE}/coreutils-${COREUTILS_SOURCE_HASH}"
COREUTILS_STORE_PATH_PACKAGE="${COREUTILS_STORE_PATH}.package"
COREUTILS_STORE_PATH_SANDBOX="${VORPAL_PATH_SANDBOX}/coreutils-${COREUTILS_SOURCE_HASH}"
COREUTILS_STORE_PATH_SOURCE="${COREUTILS_STORE_PATH}.source"
COREUTILS_VERSION="9.5"

if [ -d "${COREUTILS_STORE_PATH_PACKAGE}" ]; then
    echo "coreutils already exists"
    exit 0
fi

if [ ! -d "${COREUTILS_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
        -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
    tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$("${SCRIPT_PATH}/hash.sh" "/tmp/coreutils-${COREUTILS_VERSION}")
    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$SOURCE_HASH" != "$COREUTILS_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: ${SOURCE_HASH} != ${COREUTILS_SOURCE_HASH}"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/coreutils-${COREUTILS_VERSION}" . | zstd -o "${COREUTILS_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${COREUTILS_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${COREUTILS_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${COREUTILS_STORE_PATH_SOURCE}"

    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}"
    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
fi

mkdir -p "${COREUTILS_STORE_PATH_SANDBOX}"
cp -r "${COREUTILS_STORE_PATH_SOURCE}/." "${COREUTILS_STORE_PATH_SANDBOX}"

pushd "${COREUTILS_STORE_PATH_SANDBOX}"

./configure --prefix="${COREUTILS_STORE_PATH_PACKAGE}"
make -j"$(nproc)"
make install

popd

rm -rf "${COREUTILS_STORE_PATH_SANDBOX}"
