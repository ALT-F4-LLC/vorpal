#!/usr/bin/env bash
set -euo pipefail

COREUTILS_VERSION="9.5"

if [[ -f "${ENV_PATH}/bin/sha256sum" ]]; then
    "${ENV_PATH}/bin/sha256sum" --version | head -n 1
    exit 0
fi

curl -L \
    "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
    -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"

tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/coreutils-${COREUTILS_VERSION}"

./configure --prefix="${ENV_PATH}"
make
make install

popd

rm -rf "/tmp/coreutils-${COREUTILS_VERSION}"
rm -rf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
