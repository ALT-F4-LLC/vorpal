#!/usr/bin/env bash
set -euo pipefail

XZ_VERSION="5.8.2"

mkdir -p "${1}/bin"

if [[ -x "${1}/bin/xz" ]]; then
  "${1}/bin/xz" --version || true
  exit 0
fi

XZ_ARCHIVE="/tmp/xz-${XZ_VERSION}.tar.gz"
XZ_URL="https://github.com/tukaani-project/xz/releases/download/v${XZ_VERSION}/xz-${XZ_VERSION}.tar.gz"

echo "Downloading xz ${XZ_VERSION}..."

curl -fL "${XZ_URL}" -o "${XZ_ARCHIVE}"

TMPDIR="$(mktemp -d)"

trap 'rm -rf "${TMPDIR}" "${XZ_ARCHIVE}"' EXIT

tar -xzf "${XZ_ARCHIVE}" -C "${TMPDIR}"

cd "${TMPDIR}/xz-${XZ_VERSION}"

./configure --prefix="${TMPDIR}/install" --disable-shared --enable-static && make -j"$(nproc 2>/dev/null || sysctl -n hw.ncpu)" && make install

install -m 0755 "${TMPDIR}/install/bin/xz" "${1}/bin/xz"

"${1}/bin/xz" --version
