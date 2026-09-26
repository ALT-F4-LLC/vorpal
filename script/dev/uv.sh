#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
UV_ARCH=""
UV_OS=""
UV_SHA256=""
REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Same source of truth the SDK reads to pin the build-target uv.
UV_VERSION="$(sed -n 's/.*DEFAULT_UV_VERSION: &str = "\([^"]*\)".*/\1/p' \
    "${REPO_PATH}/sdk/rust/src/artifact/uv.rs")"

if [[ "${UV_VERSION}" == "" ]]; then
    echo "Could not read DEFAULT_UV_VERSION from sdk/rust/src/artifact/uv.rs" >&2
    exit 1
fi

case "${OS}" in
  darwin)
    UV_OS="apple-darwin"
    ;;
  linux)
    UV_OS="unknown-linux-gnu"
    ;;
  *)
    echo "Unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

case "${ARCH}" in
  x86_64|amd64)
    UV_ARCH="x86_64"
    ;;
  arm64|aarch64)
    UV_ARCH="aarch64"
    ;;
  *)
    echo "Unsupported ARCH: ${ARCH}" >&2
    exit 1
    ;;
esac

mkdir -p "${1}/bin"

if [[ -x "${1}/bin/uv" ]] && [[ "$("${1}/bin/uv" --version | cut -d' ' -f2)" == "${UV_VERSION}" ]]; then
  "${1}/bin/uv" --version
  exit 0
fi

UV_TARGET="uv-${UV_ARCH}-${UV_OS}"

case "${UV_VERSION}-${UV_TARGET}" in
  "0.10.11-uv-aarch64-apple-darwin")
    UV_SHA256="437a7d498dd6564d5bf986074249ba1fc600e73da55ae04d7bd4c24d5f149b95"
    ;;
  "0.10.11-uv-x86_64-apple-darwin")
    UV_SHA256="ff90020b554cf02ef8008535c9aab6ef27bb7be6b075359300dec79c361df897"
    ;;
  "0.10.11-uv-aarch64-unknown-linux-gnu")
    UV_SHA256="23003df007937dd607409c8ddf010baa82bad2673e60e254632ca5b04edcce13"
    ;;
  "0.10.11-uv-x86_64-unknown-linux-gnu")
    UV_SHA256="5a360b0de092ddf4131f5313d0411b48c4e95e8107e40c3f8f2e9fcb636b3583"
    ;;
esac

UV_TMPDIR="$(mktemp -d)"
UV_ARCHIVE="${UV_TMPDIR}/${UV_TARGET}-${UV_VERSION}.tar.gz"
UV_URL="https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/${UV_TARGET}.tar.gz"

trap 'rm -rf "${UV_TMPDIR}"' EXIT

echo "Downloading uv ${UV_VERSION} (${UV_OS}/${UV_ARCH})..."

curl -fL "${UV_URL}" -o "${UV_ARCHIVE}"

verify_sha256 "${UV_ARCHIVE}" "${UV_SHA256}"

tar -xzf "${UV_ARCHIVE}" -C "${UV_TMPDIR}"

install -m 0755 "${UV_TMPDIR}/${UV_TARGET}/uv" "${1}/bin/uv"

"${1}/bin/uv" --version
