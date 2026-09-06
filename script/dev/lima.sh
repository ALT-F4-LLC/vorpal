#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

export PATH="${1}/bin:${PATH}"

if [[ -f "${1}/bin/lima" ]]; then
    "${1}/bin/lima" --version
    exit 0
fi

# Pinned rather than resolved from `releases/latest`: an attacker-influenced
# version response leaves nothing for a digest pin to key on.
LIMA_VERSION="2.2.0"
LIMA_TARGET="$(uname -s)-$(uname -m)"
LIMA_SHA256=""

case "${LIMA_VERSION}-${LIMA_TARGET}" in
    "2.2.0-Darwin-arm64")
        LIMA_SHA256="bbdef91774885a0d05f7b048c4eb89ae2bcf3a0c252ae7ca7934e63df76d93c3"
        ;;
    "2.2.0-Darwin-x86_64")
        LIMA_SHA256="0d6f99c19f6e4bc3c92730c4c29d929e6927f0cb0a0ba1a84383367135a8ff31"
        ;;
    "2.2.0-Linux-x86_64")
        LIMA_SHA256="a0ea1ccf6b7335a900adb5f8d2b8384457965fecb1ba72f09b4e3e46d12f424a"
        ;;
esac

LIMA_TMPDIR="$(mktemp -d)"
LIMA_ARCHIVE="${LIMA_TMPDIR}/lima-${LIMA_VERSION}-${LIMA_TARGET}.tar.gz"

trap 'rm -rf "${LIMA_TMPDIR}"' EXIT

curl -fsSL "https://github.com/lima-vm/lima/releases/download/v${LIMA_VERSION}/lima-${LIMA_VERSION}-${LIMA_TARGET}.tar.gz" -o "${LIMA_ARCHIVE}"

verify_sha256 "${LIMA_ARCHIVE}" "${LIMA_SHA256}"

tar -xzvm -C "${1}" -f "${LIMA_ARCHIVE}"
