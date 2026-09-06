#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
BUN_ARCH=""
BUN_OS=""
BUN_SHA256=""
REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Same source of truth CI reads to pin `setup-bun`.
BUN_VERSION="$(sed -n 's/.*DEFAULT_BUN_VERSION: &str = "\([^"]*\)".*/\1/p' \
    "${REPO_PATH}/sdk/rust/src/artifact/bun.rs")"

if [[ "${BUN_VERSION}" == "" ]]; then
    echo "Could not read DEFAULT_BUN_VERSION from sdk/rust/src/artifact/bun.rs" >&2
    exit 1
fi

case "${OS}" in
  darwin|linux)
    BUN_OS="${OS}"
    ;;
  *)
    echo "Unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

case "${ARCH}" in
  x86_64|amd64)
    BUN_ARCH="x64"
    ;;
  arm64|aarch64)
    BUN_ARCH="aarch64"
    ;;
  *)
    echo "Unsupported ARCH: ${ARCH}" >&2
    exit 1
    ;;
esac

mkdir -p "${1}/bin"

if [[ -x "${1}/bin/bun" ]] && [[ "$("${1}/bin/bun" --version)" == "${BUN_VERSION}" ]]; then
  "${1}/bin/bun" --version
  exit 0
fi

case "${BUN_VERSION}-${BUN_OS}-${BUN_ARCH}" in
  "1.3.10-darwin-aarch64")
    BUN_SHA256="82034e87c9d9b4398ea619aee2eed5d2a68c8157e9a6ae2d1052d84d533ccd8d"
    ;;
  "1.3.10-darwin-x64")
    BUN_SHA256="c1d90bf6140f20e572c473065dc6b37a4b036349b5e9e4133779cc642ad94323"
    ;;
  "1.3.10-linux-aarch64")
    BUN_SHA256="fa5ecb25cafa8e8f5c87a0f833719d46dd0af0a86c7837d806531212d55636d3"
    ;;
  "1.3.10-linux-x64")
    BUN_SHA256="f57bc0187e39623de716ba3a389fda5486b2d7be7131a980ba54dc7b733d2e08"
    ;;
esac

BUN_TARGET="bun-${BUN_OS}-${BUN_ARCH}"
BUN_TMPDIR="$(mktemp -d)"
BUN_ARCHIVE="${BUN_TMPDIR}/${BUN_TARGET}-${BUN_VERSION}.zip"
BUN_URL="https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/${BUN_TARGET}.zip"

trap 'rm -rf "${BUN_TMPDIR}"' EXIT

echo "Downloading Bun ${BUN_VERSION} (${BUN_OS}/${BUN_ARCH})..."

curl -fL "${BUN_URL}" -o "${BUN_ARCHIVE}"

verify_sha256 "${BUN_ARCHIVE}" "${BUN_SHA256}"

unzip -q "${BUN_ARCHIVE}" -d "${BUN_TMPDIR}"

install -m 0755 "${BUN_TMPDIR}/${BUN_TARGET}/bun" "${1}/bin/bun"

"${1}/bin/bun" --version
