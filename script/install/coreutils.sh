#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <sandbox_path>"
    exit 1
fi

echo "Install coreutils -> $1"

COREUTILS_VERSION="9.5"
SANDBOX_PATH="$1"

curl -L \
    "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
    -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"

tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/coreutils-${COREUTILS_VERSION}"

./configure --prefix="${SANDBOX_PATH}"

make

make install

popd

rm -rf "/tmp/coreutils-${COREUTILS_VERSION}"

rm -rf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
