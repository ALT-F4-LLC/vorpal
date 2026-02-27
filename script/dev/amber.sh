#!/usr/bin/env bash
set -euo pipefail

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
TEMP_FILE="/tmp/amber-${AMBER_TARGET}.tar.xz"

echo "Downloading amber ${AMBER_VERSION} for ${AMBER_TARGET}"
curl -fL -o "${TEMP_FILE}" "${AMBER_URL}"

xz -d "${TEMP_FILE}"
tar -xf "/tmp/amber-${AMBER_TARGET}.tar" -C "${1}/bin"

rm "/tmp/amber-${AMBER_TARGET}.tar"

chmod +x "${1}/bin/amber"
