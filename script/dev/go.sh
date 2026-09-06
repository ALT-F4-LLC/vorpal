#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
GO_ARCH=""
GO_OS=""
REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Same source of truth CI reads through `setup-go`'s `go-version-file`.
GO_MOD="${REPO_PATH}/sdk/go/go.mod"
GO_DIRECTIVE_LINE="$(awk '/^go /{print NR; exit}' "${GO_MOD}")"
GO_VERSION="$(awk '/^toolchain go[0-9]/{sub(/^go/, "", $2); print $2; exit}' "${GO_MOD}")"

if [[ "${GO_VERSION}" == "" ]]; then
    GO_VERSION="$(awk '/^go /{print $2; exit}' "${GO_MOD}")"
fi

if [[ "${GO_VERSION}" == "" ]]; then
    echo "Could not read the go directive from sdk/go/go.mod" >&2
    exit 1
fi

# A two-component directive is a minor-line floor, the same way `setup-go`
# reads it. go.dev release archives are only published under exact patch
# versions, so resolve the newest stable patch of that line.
if [[ "${GO_VERSION}" =~ ^[0-9]+\.[0-9]+$ ]]; then
    GO_RELEASES="$(curl -fsSL 'https://go.dev/dl/?mode=json&include=all')" || GO_RELEASES=""
    GO_VERSION="$(jq -r --arg prefix "go${GO_VERSION}." \
        '[.[] | select(.stable and (.version | startswith($prefix))) | .version][0] // empty' \
        <<<"${GO_RELEASES:-[]}" | sed 's/^go//')"
fi

if [[ "${GO_VERSION}" == "" ]]; then
    echo "Could not resolve a released Go patch for sdk/go/go.mod:${GO_DIRECTIVE_LINE}" >&2
    echo "Add a toolchain directive pinning an exact version, or retry with network access to go.dev." >&2
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
