#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

XZ_SHA256=""
XZ_VERSION="5.8.2"

mkdir -p "${1}/bin"

if [[ -x "${1}/bin/xz" ]]; then
  "${1}/bin/xz" --version || true
  exit 0
fi

# The release tarball is platform-independent source, so the pin keys on
# version alone.
case "${XZ_VERSION}" in
  "5.8.2")
    XZ_SHA256="ce09c50a5962786b83e5da389c90dd2c15ecd0980a258dd01f70f9e7ce58a8f1"
    ;;
esac

XZ_TMPDIR="$(mktemp -d)"
XZ_ARCHIVE="${XZ_TMPDIR}/xz-${XZ_VERSION}.tar.gz"
XZ_URL="https://github.com/tukaani-project/xz/releases/download/v${XZ_VERSION}/xz-${XZ_VERSION}.tar.gz"

trap 'rm -rf "${XZ_TMPDIR}"' EXIT

echo "Downloading xz ${XZ_VERSION}..."

curl -fL "${XZ_URL}" -o "${XZ_ARCHIVE}"

verify_sha256 "${XZ_ARCHIVE}" "${XZ_SHA256}"

tar -xzf "${XZ_ARCHIVE}" -C "${XZ_TMPDIR}"

cd "${XZ_TMPDIR}/xz-${XZ_VERSION}"

./configure --prefix="${XZ_TMPDIR}/install" --disable-shared --enable-static && make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)" && make install

install -m 0755 "${XZ_TMPDIR}/install/bin/xz" "${1}/bin/xz"

"${1}/bin/xz" --version
