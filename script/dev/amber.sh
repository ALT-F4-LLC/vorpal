#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

export PATH="${1}/bin:${PATH}"

if [[ -f "${1}/bin/amber" ]]; then
    "${1}/bin/amber" --version
    exit 0
fi

# Detect architecture and OS
ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
AMBER_VERSION="0.5.1"

case "${ARCH}" in
    "x86_64")
        AMBER_ARCH="x86_64"
        ;;
    "arm64"|"aarch64")
        AMBER_ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

case "${OS}" in
    "darwin")
        AMBER_OS="macos"
        ;;
    "linux")
        AMBER_OS="linux-gnu"
        ;;
    *)
        echo "Unsupported OS: ${OS}"
        exit 1
        ;;
esac

# Build the download URL and filenames
AMBER_TARGET="${AMBER_OS}-${AMBER_ARCH}"
AMBER_URL="https://github.com/amber-lang/amber/releases/download/${AMBER_VERSION}-alpha/amber-${AMBER_TARGET}.tar.xz"

AMBER_SHA256=""

case "${AMBER_VERSION}-${AMBER_TARGET}" in
    "0.5.1-macos-aarch64")
        AMBER_SHA256="987deea74527692c0f9e30036253c55b1d2c023dfcd46d6def67644350c6a424"
        ;;
    "0.5.1-macos-x86_64")
        AMBER_SHA256="0332351ede91f50795dcbfc28aa3fcf4445da597fe475d223b85fb6ffd0a649b"
        ;;
    "0.5.1-linux-gnu-aarch64")
        AMBER_SHA256="c1ecad1e98404fd0e1d8817e11feeabb8cc3061d1229d5b963ba204fad3ed671"
        ;;
    "0.5.1-linux-gnu-x86_64")
        AMBER_SHA256="4deaaa2d63aa4addcf8514efa11446f76ac998b7ccbb290eb821966c82992729"
        ;;
esac

AMBER_TMPDIR="$(mktemp -d)"
AMBER_ARCHIVE="${AMBER_TMPDIR}/amber-${AMBER_TARGET}.tar.xz"

trap 'rm -rf "${AMBER_TMPDIR}"' EXIT

echo "Downloading amber ${AMBER_VERSION} for ${AMBER_TARGET}"
curl -fL -o "${AMBER_ARCHIVE}" "${AMBER_URL}"

verify_sha256 "${AMBER_ARCHIVE}" "${AMBER_SHA256}"

xz -d "${AMBER_ARCHIVE}"
tar -xf "${AMBER_TMPDIR}/amber-${AMBER_TARGET}.tar" -C "${1}/bin"

chmod +x "${1}/bin/amber"
