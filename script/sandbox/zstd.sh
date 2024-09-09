#!/usr/bin/env bash
set -euo pipefail

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

SANDBOX_PACKAGE_PATH="$1"

# Environment variables
CPU_COUNT=""
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VORPAL_PATH="/var/lib/vorpal"

# Build variables
ZSTD_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/zstd")"
ZSTD_STORE_PATH="${VORPAL_PATH}/store/zstd-${ZSTD_SOURCE_HASH}"
ZSTD_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/zstd-${ZSTD_SOURCE_HASH}"
ZSTD_STORE_PATH_SOURCE="${ZSTD_STORE_PATH}.source"
ZSTD_VERSION="1.5.5"

if [[ "${OS}" == "darwin" ]]; then
    CPU_COUNT="-j$(sysctl -n hw.ncpu)"
fi

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

if [ ! -d "${ZSTD_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://github.com/facebook/zstd/releases/download/v${ZSTD_VERSION}/zstd-${ZSTD_VERSION}.tar.gz" \
        -o "/tmp/zstd-${ZSTD_VERSION}.tar.gz"
    tar -xzf "/tmp/zstd-${ZSTD_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/zstd-${ZSTD_VERSION}")
    echo "Calculated source hash: $SOURCE_HASH"

    if [ "$SOURCE_HASH" != "$ZSTD_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: $SOURCE_HASH != $ZSTD_SOURCE_HASH"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/zstd-${ZSTD_VERSION}" . | zstd -o "${ZSTD_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${ZSTD_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${ZSTD_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${ZSTD_STORE_PATH_SOURCE}"

    rm -rf "/tmp/zstd-${ZSTD_VERSION}"
    rm -rf "/tmp/zstd-${ZSTD_VERSION}.tar.gz"
fi

rm -rf "${ZSTD_STORE_PATH_SANDBOX}" || true

mkdir -p "${ZSTD_STORE_PATH_SANDBOX}"

cp -r "${ZSTD_STORE_PATH_SOURCE}/." "${ZSTD_STORE_PATH_SANDBOX}"

pushd "${ZSTD_STORE_PATH_SANDBOX}"

make ${CPU_COUNT}
make install PREFIX="${SANDBOX_PACKAGE_PATH}"

popd

rm -rf "${ZSTD_STORE_PATH_SANDBOX}"
