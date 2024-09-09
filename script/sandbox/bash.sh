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
BASH_SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/bash")"
BASH_STORE_PATH="${VORPAL_PATH}/store/bash-${BASH_SOURCE_HASH}"
BASH_STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/bash-${BASH_SOURCE_HASH}"
BASH_STORE_PATH_SOURCE="${BASH_STORE_PATH}.source"
BASH_VERSION="5.2"

if [[ "${OS}" == "darwin" ]]; then
    CPU_COUNT="-j$(sysctl -n hw.ncpu)"
fi

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

if [ ! -d "${BASH_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz" \
        -o "/tmp/bash-${BASH_VERSION}.tar.gz"
    tar -xvzf "/tmp/bash-${BASH_VERSION}.tar.gz" -C "/tmp"

    echo "Calculating source hash..."

    SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/bash-${BASH_VERSION}")

    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$SOURCE_HASH" != "$BASH_SOURCE_HASH" ]; then
        echo "Source hash mismatch: ${SOURCE_HASH} != ${BASH_SOURCE_HASH}"
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

./configure --prefix="${SANDBOX_PACKAGE_PATH}"
make -j${CPU_COUNT}
make install

popd

rm -rf "${BASH_STORE_PATH_SANDBOX}"
