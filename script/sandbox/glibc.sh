#!/usr/bin/env bash
set -euo pipefail

VERSION="2.40"

if [ -z "$1" ]; then
  echo "Usage: $0 <sandbox-package-path>"
  exit 1
fi

curl -L \
    "https://ftp.gnu.org/gnu/glibc/glibc-${VERSION}.tar.gz" \
    -o "/tmp/glibc-${VERSION}.tar.gz"

tar -xzf "/tmp/glibc-${VERSION}.tar.gz" -C "/tmp"

pushd "/tmp/glibc-${VERSION}"

# case $(uname -m) in
#     i?86)   ln -sfv ld-linux.so.2 $LFS/lib/ld-lsb.so.3
#     ;;
#     x86_64) ln -sfv ../lib/ld-linux-x86-64.so.2 $LFS/lib64
#             ln -sfv ../lib/ld-linux-x86-64.so.2 $LFS/lib64/ld-lsb-x86-64.so.3
#     ;;
# esac

mkdir -p ./build

popd

pushd "/tmp/glibc-${VERSION}/build"

../configure --prefix="${1}" libc_cv_slibdir="${1}/lib"

make -j$(nproc)

make install

popd

rm -rf "/tmp/glibc-${VERSION}"

rm -rf "/tmp/glibc-${VERSION}.tar.gz"
