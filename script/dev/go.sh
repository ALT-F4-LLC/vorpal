#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
GO_ARCH=""
GO_OS=""
REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Same source of truth CI reads through `setup-go`'s `go-version-file`.
GO_VERSION="$(awk '/^go /{print $2; exit}' "${REPO_PATH}/sdk/go/go.mod")"

if [[ "${GO_VERSION}" == "" ]]; then
    echo "Could not read the go directive from sdk/go/go.mod" >&2
    exit 1
fi

case "${OS}" in
  darwin|linux)
    GO_OS="${OS}"
    ;;
  *)
    echo "Unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

case "${ARCH}" in
  x86_64|amd64)
    GO_ARCH="amd64"
    ;;
  arm64|aarch64)
    GO_ARCH="arm64"
    ;;
  *)
    echo "Unsupported ARCH: ${ARCH}" >&2
    exit 1
    ;;
esac

mkdir -p "${1}/bin"

if [[ -x "${1}/go/bin/go" ]] && [[ "$("${1}/go/bin/go" env GOVERSION)" == "go${GO_VERSION}" ]]; then
  "${1}/go/bin/go" version
  exit 0
fi

GO_TMPDIR="$(mktemp -d)"
GO_ARCHIVE="${GO_TMPDIR}/go${GO_VERSION}.${GO_OS}-${GO_ARCH}.tar.gz"
GO_URL="https://go.dev/dl/go${GO_VERSION}.${GO_OS}-${GO_ARCH}.tar.gz"

trap 'rm -rf "${GO_TMPDIR}"' EXIT

echo "Downloading Go ${GO_VERSION} (${GO_OS}/${GO_ARCH})..."

curl -fL "${GO_URL}" -o "${GO_ARCHIVE}"

rm -rf "${1}/go"

tar -xzf "${GO_ARCHIVE}" -C "${1}"

ln -sf "../go/bin/go" "${1}/bin/go"
ln -sf "../go/bin/gofmt" "${1}/bin/gofmt"

"${1}/bin/go" version
