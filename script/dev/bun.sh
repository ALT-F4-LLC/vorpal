#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
BUN_ARCH=""
BUN_OS=""
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

BUN_TARGET="bun-${BUN_OS}-${BUN_ARCH}"
BUN_TMPDIR="$(mktemp -d)"
BUN_ARCHIVE="${BUN_TMPDIR}/${BUN_TARGET}-${BUN_VERSION}.zip"
BUN_URL="https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/${BUN_TARGET}.zip"

trap 'rm -rf "${BUN_TMPDIR}"' EXIT

echo "Downloading Bun ${BUN_VERSION} (${BUN_OS}/${BUN_ARCH})..."

curl -fL "${BUN_URL}" -o "${BUN_ARCHIVE}"

unzip -q "${BUN_ARCHIVE}" -d "${BUN_TMPDIR}"

install -m 0755 "${BUN_TMPDIR}/${BUN_TARGET}/bun" "${1}/bin/bun"

"${1}/bin/bun" --version
