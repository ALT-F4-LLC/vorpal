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
COREUTILS_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/coreutils")"
COREUTILS_STORE_PATH="${VORPAL_PATH}/store/coreutils-${COREUTILS_SOURCE_HASH}"
COREUTILS_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/coreutils-${COREUTILS_SOURCE_HASH}"
COREUTILS_STORE_PATH_SOURCE="${COREUTILS_STORE_PATH}.source"
COREUTILS_VERSION="9.5"

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

if [ ! -d "${COREUTILS_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
        -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
    tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

    echo "Calculating source hash..."

    SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/coreutils-${COREUTILS_VERSION}")

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

rm -rf "${COREUTILS_STORE_PATH_SANDBOX}" || true

mkdir -p "${COREUTILS_STORE_PATH_SANDBOX}"

cp -r "${COREUTILS_STORE_PATH_SOURCE}/." "${COREUTILS_STORE_PATH_SANDBOX}"

pushd "${COREUTILS_STORE_PATH_SANDBOX}"

./configure --prefix="${SANDBOX_PACKAGE_PATH}"
make ${CPU_COUNT}
make install

popd

rm -rf "${COREUTILS_STORE_PATH_SANDBOX}"
