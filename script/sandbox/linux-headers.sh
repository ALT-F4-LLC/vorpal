#!/usr/bin/env bash
set -euo pipefail

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

SANDBOX_PACKAGE_PATH="$1"

# Environment variables
ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VORPAL_PATH="/var/lib/vorpal"

if [[ "${ARCH}" == "arm64" ]]; then
    ARCH="aarch64"
fi

# Build variables
SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${ARCH}-${OS}/linux-headers")"
STORE_PATH="${VORPAL_PATH}/store/linux-headers-${SOURCE_HASH}"
STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/linux-headers-${SOURCE_HASH}"
STORE_PATH_SOURCE="${STORE_PATH}.source"
VERSION="6.10.8"

if [ ! -d "${STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${VERSION}.tar.xz" \
        -o "/tmp/linux-${VERSION}.tar.xz"
    tar -xvf "/tmp/linux-${VERSION}.tar.xz" -C "/tmp"

    echo "Calculating source hash..."

    SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/linux-${VERSION}")

    echo "Calculated source hash: ${SOURCE_HASH}"

    if [ "$SOURCE_HASH" != "$SOURCE_HASH" ]; then
        echo "Download source hash mismatch: ${SOURCE_HASH} != ${SOURCE_HASH}"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/linux-${VERSION}" . | zstd -o "${STORE_PATH_SOURCE}.tar.zst"

    mkdir -p "${STORE_PATH_SOURCE}"

    zstd --decompress --stdout "${STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${STORE_PATH_SOURCE}"

    rm -rf "/tmp/linux-${VERSION}"
    rm -rf "/tmp/linux-${VERSION}.tar.gz"
fi

rm -rf "${STORE_PATH_SANDBOX}" || true

mkdir -p "${STORE_PATH_SANDBOX}"

cp -r "${STORE_PATH_SOURCE}/." "${STORE_PATH_SANDBOX}"

pushd "${STORE_PATH_SANDBOX}"

make mrproper
make headers

find usr/include -type f ! -name '*.h' -delete

mkdir -p "${SANDBOX_PACKAGE_PATH}/usr"

cp -rv usr/include "${SANDBOX_PACKAGE_PATH}/usr"

popd

rm -rf "${STORE_PATH_SANDBOX}"
