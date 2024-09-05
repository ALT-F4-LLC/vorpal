#!/usr/bin/env bash
set -euo pipefail

GCC_SOURCE_HASH="$(cat "${SCRIPT_PATH_INSTALL}/gcc.sha256sum")"
GCC_STORE_PATH="${VORPAL_PATH_STORE}/gcc-${GCC_SOURCE_HASH}"
GCC_STORE_PATH_PACKAGE="${GCC_STORE_PATH}.package"
GCC_STORE_PATH_SANDBOX="${VORPAL_PATH_SANDBOX}/gcc-${GCC_SOURCE_HASH}"
GCC_STORE_PATH_SOURCE="${GCC_STORE_PATH}.source"
GCC_VERSION="14.2.0"
# PATH="${PREFIX_PATH}/bin:${PATH}"

if [ -d "${GCC_STORE_PATH_PACKAGE}" ]; then
    echo "gcc already exists"
    exit 0
fi

if [ ! -d "${GCC_STORE_PATH_SOURCE}" ]; then
    curl -L \
        "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.gz" \
        -o "/tmp/gcc-${GCC_VERSION}.tar.gz"
    tar -xzf "/tmp/gcc-${GCC_VERSION}.tar.gz" -C "/tmp"

    ## TODO: move hash as arg to script

    echo "Calculating source hash..."
    SOURCE_HASH=$("${SCRIPT_PATH}/hash.sh" "/tmp/gcc-${GCC_VERSION}")
    echo "Calculated source hash: $SOURCE_HASH"

    if [ "$SOURCE_HASH" != "$GCC_SOURCE_HASH" ]; then
        echo "Download source hash mismatch: $SOURCE_HASH != $GCC_SOURCE_HASH"
        exit 1
    fi

    ## TODO: move to separate script

    tar -cvf - -C "/tmp/gcc-${GCC_VERSION}" . | zstd -o "${GCC_STORE_PATH_SOURCE}.tar.zst"
    mkdir -p "${GCC_STORE_PATH_SOURCE}"
    zstd --decompress --stdout "${GCC_STORE_PATH_SOURCE}.tar.zst" | tar -xvf - -C "${GCC_STORE_PATH_SOURCE}"

    rm -rf "/tmp/gcc-${GCC_VERSION}"
    rm -rf "/tmp/gcc-${GCC_VERSION}.tar.gz"
fi

mkdir -p "${GCC_STORE_PATH_SANDBOX}"
cp -r "${GCC_STORE_PATH_SOURCE}/." "${GCC_STORE_PATH_SANDBOX}"

pushd "${GCC_STORE_PATH_SANDBOX}"

./contrib/download_prerequisites

mkdir -p ./build

popd

pushd "${GCC_STORE_PATH_SANDBOX}/build"

../configure --enable-languages="c,c++" --prefix="${GCC_STORE_PATH_PACKAGE}"

make -j"$(nproc)"
make install

popd

rm -rf "${GCC_STORE_PATH_SANDBOX}"
