#!/usr/bin/env bash
set -euo pipefail

ZSTD_VERSION="1.5.5"

if [[ -f "${ENV_PATH}/bin/zstd" ]]; then
    "${ENV_PATH}/bin/zstd" --version
    exit 0
fi

curl -L \
    "https://github.com/facebook/zstd/releases/download/v${ZSTD_VERSION}/zstd-${ZSTD_VERSION}.tar.gz" \
    -o "/tmp/zstd-${ZSTD_VERSION}.tar.gz"

tar -xzf "/tmp/zstd-${ZSTD_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/zstd-${ZSTD_VERSION}"

make -j"$(sysctl -n hw.ncpu)"

make install PREFIX="${ENV_PATH}"

popd

rm -rf "/tmp/zstd-${ZSTD_VERSION}"
rm -rf "/tmp/zstd-${ZSTD_VERSION}.tar.gz"
