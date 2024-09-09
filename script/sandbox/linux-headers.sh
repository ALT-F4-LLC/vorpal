#!/usr/bin/env bash
set -euo pipefail

VERSION="6.10.8"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

curl -L \
    "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${VERSION}.tar.xz" \
    -o "/tmp/linux-${VERSION}.tar.xz"

tar -xvf "/tmp/linux-${VERSION}.tar.xz" -C "/tmp"

pushd "/tmp/linux-${VERSION}"

make mrproper

make headers

find usr/include -type f ! -name '*.h' -delete

mkdir -p "${1}/usr"

cp -rv usr/include "${1}/usr"

popd

rm -rf "/tmp/linux-${VERSION}"

rm -rf "/tmp/linux-${VERSION}.tar.gz"
