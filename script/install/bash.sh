#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <sandbox_path>"
    exit 1
fi

echo "Install bash -> $1"

BASH_VERSION="5.2"
SANDBOX_PATH="$1"

curl -L \
    "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz" \
    -o "/tmp/bash-${BASH_VERSION}.tar.gz"

tar -xzf "/tmp/bash-${BASH_VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/bash-${BASH_VERSION}"

./configure --prefix="${SANDBOX_PATH}"

make

make install

popd

rm -rf "/tmp/bash-${BASH_VERSION}"

rm -f "/tmp/bash-${BASH_VERSION}.tar.gz"
