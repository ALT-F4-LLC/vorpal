#!/usr/bin/env bash
set -euo pipefail

BASH_SOURCE_HASH="$(cat "${SCRIPT_PATH_INSTALL}/bash.sha256sum")"
BASH_STORE_PATH="${VORPAL_PATH_STORE}/bash-${BASH_SOURCE_HASH}"
BASH_STORE_PATH_PACKAGE="${BASH_STORE_PATH}.package"
BASH_STORE_PATH_SANDBOX="${VORPAL_PATH_SANDBOX}/bash-${BASH_SOURCE_HASH}"
BASH_STORE_PATH_SOURCE="${BASH_STORE_PATH}.source"
BASH_VERSION="5.2"

if [ -d "${BASH_STORE_PATH_PACKAGE}" ]; then
    echo "bash already exists"
    exit 0
fi

if [ ! -d "${BASH_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz" \
        -o "/tmp/bash-${BASH_VERSION}.tar.gz"
    tar -xvzf "/tmp/bash-${BASH_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$("${SCRIPT_PATH}/hash.sh" "/tmp/bash-${BASH_VERSION}")
    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$SOURCE_HASH" != "$BASH_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: ${SOURCE_HASH} != ${BASH_SOURCE_HASH}"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/bash-${BASH_VERSION}" . | zstd -o "${BASH_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${BASH_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${BASH_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${BASH_STORE_PATH_SOURCE}"

    rm -rf "/tmp/bash-${BASH_VERSION}"
    rm -rf "/tmp/bash-${BASH_VERSION}.tar.gz"
fi

mkdir -p "${BASH_STORE_PATH_SANDBOX}"
cp -r "${BASH_STORE_PATH_SOURCE}/." "${BASH_STORE_PATH_SANDBOX}"

pushd "${BASH_STORE_PATH_SANDBOX}"

./configure --prefix="${BASH_STORE_PATH_PACKAGE}"
make -j"$(nproc)"
make install

popd

rm -rf "${BASH_STORE_PATH_SANDBOX}"
