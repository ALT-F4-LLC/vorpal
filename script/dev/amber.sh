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

# Map architecture and OS to amber release naming
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
        AMBER_OS="apple-darwin"
        ;;
    "linux")
        AMBER_OS="unknown-linux-gnu"
        ;;
    *)
        echo "Unsupported OS: ${OS}"
        exit 1
        ;;
esac

# Build the download URL and filenames
AMBER_TARGET="${AMBER_ARCH}-${AMBER_OS}"
AMBER_URL="https://github.com/amber-lang/amber/releases/download/0.4.0-alpha/amber-${AMBER_TARGET}.tar.xz"
TEMP_FILE="/tmp/amber-${AMBER_TARGET}.tar.xz"

echo "Downloading amber for ${AMBER_TARGET}"
curl -L -o "${TEMP_FILE}" "${AMBER_URL}"

tar -xf "${TEMP_FILE}" -C "${1}/bin"

# Handle different archive structures
if [[ "${AMBER_TARGET}" == "aarch64-unknown-linux-gnu" ]]; then
    # This archive contains the binary directly, no subdirectory
    # Binary should already be extracted to ${1}/bin/amber
    if [[ ! -f "${1}/bin/amber" ]]; then
        echo "Error: amber binary not found after extraction"
        exit 1
    fi
else
    # Other archives contain the binary in a subdirectory
    mv "${1}/bin/amber-${AMBER_TARGET}/amber" "${1}/bin/amber"
    # Clean up extracted directory
    rm -rf "${1}/bin/amber-${AMBER_TARGET}"
fi

rm "${TEMP_FILE}"

chmod +x "${1}/bin/amber"
