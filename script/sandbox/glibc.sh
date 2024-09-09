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
SOURCE_HASH="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/glibc")"
STORE_PATH="${VORPAL_PATH}/store/glibc-${SOURCE_HASH}"
STORE_PATH_SANDBOX="${VORPAL_PATH}/sandbox/glibc-${SOURCE_HASH}"
STORE_PATH_SOURCE="${STORE_PATH}.source"
VERSION="2.40"

if [ ! -d "${STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/glibc/glibc-${VERSION}.tar.gz" \
        -o "/tmp/glibc-${VERSION}.tar.gz"
    tar -xzf "/tmp/glibc-${VERSION}.tar.gz" -C "/tmp"

    echo "Calculating source hash..."

    DOWNLOAD_SOURCE_HASH=$("${PWD}/script/hash_path.sh" "/tmp/glibc-${VERSION}")

    echo "Calculated source hash: ${DOWNLOAD_SOURCE_HASH}"

    if [ "$DOWNLOAD_SOURCE_HASH" != "$SOURCE_HASH" ]; then
        echo "Download source hash mismatch: ${SOURCE_HASH} != ${SOURCE_HASH}"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/glibc-${VERSION}" . | zstd -o "${STORE_PATH_SOURCE}.tar.zst"

    mkdir -p "${STORE_PATH_SOURCE}"

    zstd --decompress --stdout "${STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${STORE_PATH_SOURCE}"

    rm -rf "/tmp/glibc-${VERSION}"
    rm -rf "/tmp/glibc-${VERSION}.tar.gz"
fi

mkdir -p "${STORE_PATH_SANDBOX}"

cp -r "${STORE_PATH_SOURCE}/." "${STORE_PATH_SANDBOX}"

pushd "${STORE_PATH_SANDBOX}"

# case $(uname -m) in
#     i?86)   ln -sfv ld-linux.so.2 $LFS/lib/ld-lsb.so.3
#     ;;
#     x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $LFS/lib64
#             ln -sfv ../lib/ld-linux-x86-64.so.2 $LFS/lib64/ld-lsb-x86-64.so.3
#     ;;
# esac

mkdir -p ./build

popd

pushd "${STORE_PATH_SANDBOX}/build"

../configure \
    --prefix="${SANDBOX_PACKAGE_PATH}" \
    libc_cv_slibdir="${SANDBOX_PACKAGE_PATH}/lib"

make -j$(nproc)
make install

popd

rm -rf "${STORE_PATH_SANDBOX}"
