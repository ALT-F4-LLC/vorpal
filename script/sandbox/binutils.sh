#!/usr/bin/env bash
set -euxo pipefail

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
PACKAGE_NAME="binutils"
SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/${PACKAGE_NAME}")"
STORE_PATH="${VORPAL_PATH}/store/${PACKAGE_NAME}-${SOURCE_HASH}"
STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/${PACKAGE_NAME}-${SOURCE_HASH}"
STORE_PATH_SOURCE="${STORE_PATH}.source"
VERSION="2.43.1"

if [[ "${OS}" == "linux" ]]; then
    CPU_COUNT="-j$(nproc)"
fi

if [ ! -d "${STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/binutils/binutils-${VERSION}.tar.gz" \
        -o "/tmp/binutils-${VERSION}.tar.gz"
    tar -xvzf "/tmp/binutils-${VERSION}.tar.gz" -C "/tmp"

    echo "Calculating source hash..."

    DOWNLOAD_SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/binutils-${VERSION}")

    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$DOWNLOAD_SOURCE_HASH" != "$SOURCE_HASH" ]; then
        echo "Download hash mismatch: ${DOWNLOAD_SOURCE_HASH} != ${SOURCE_HASH}"
        exit 1
    fi

    tar -cvf - -C "/tmp/binutils-${VERSION}" . | zstd -o "${STORE_PATH_SOURCE}.tar.zst"

    mkdir -p "${STORE_PATH_SOURCE}"

    zstd --decompress --stdout "${STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${STORE_PATH_SOURCE}"

    rm -rf "/tmp/binutils-${VERSION}"
    rm -rf "/tmp/binutils-${VERSION}.tar.gz"
fi

rm -rf "${STORE_PATH_SANDBOX}" || true

mkdir -p "${STORE_PATH_SANDBOX}"

cp -r "${STORE_PATH_SOURCE}/." "${STORE_PATH_SANDBOX}"

pushd "${STORE_PATH_SANDBOX}"

mkdir -p ./build

popd

pushd "${STORE_PATH_SANDBOX}/build"

../configure --prefix="${SANDBOX_PACKAGE_PATH}"
make ${CPU_COUNT}
make install

popd

rm -rf "${STORE_PATH_SANDBOX}"
