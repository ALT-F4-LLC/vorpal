#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
GO_ARCH=""
GO_OS=""
GO_SHA256=""
REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# The exact patch installed for the minor line sdk/go/go.mod declares. A
# floating patch resolved from go.dev leaves nothing for a digest pin to key
# on, so the patch is chosen here and go.mod's line is checked against it.
GO_PINNED_VERSION="1.26.8"

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
# versions, so use the patch pinned here for that line.
if [[ "${GO_VERSION}" =~ ^[0-9]+\.[0-9]+$ ]]; then
    if [[ "${GO_PINNED_VERSION}" != "${GO_VERSION}."* ]]; then
        echo "sdk/go/go.mod:${GO_DIRECTIVE_LINE} declares go ${GO_VERSION}, but script/dev/go.sh pins ${GO_PINNED_VERSION}" >&2
        echo "Update GO_PINNED_VERSION and its checksums together with the go directive." >&2
        exit 1
    fi

    GO_VERSION="${GO_PINNED_VERSION}"
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

if [[ -x "${1}/bin/go" ]] && [[ -x "${1}/bin/gofmt" ]] &&
   [[ "$("${1}/bin/go" env GOVERSION)" == "go${GO_VERSION}" ]]; then
  "${1}/bin/go" version
  exit 0
fi

case "${GO_VERSION}.${GO_OS}-${GO_ARCH}" in
  "1.26.8.darwin-amd64")
    GO_SHA256="186be014105aa6542b767d2c6ed5cca10a0214bdff809ef1724022a8c7894150"
    ;;
  "1.26.8.darwin-arm64")
    GO_SHA256="a012b25b571bd0138a03dcd25375ceba866fe5ca822f426d2c66a4de56fd3f4b"
    ;;
  "1.26.8.linux-amd64")
    GO_SHA256="d0f743b33e8d8945e6b1f432edd15785c70507121d6e2a723b21285eddf8b57b"
    ;;
  "1.26.8.linux-arm64")
    GO_SHA256="211ffced9dcb9633a55eac6364816ec0ddd951389a740e88fa8b3337971bdda0"
    ;;
esac

GO_TMPDIR="$(mktemp -d)"
GO_ARCHIVE="${GO_TMPDIR}/go${GO_VERSION}.${GO_OS}-${GO_ARCH}.tar.gz"
GO_URL="https://go.dev/dl/go${GO_VERSION}.${GO_OS}-${GO_ARCH}.tar.gz"

trap 'rm -rf "${GO_TMPDIR}"' EXIT

echo "Downloading Go ${GO_VERSION} (${GO_OS}/${GO_ARCH})..."

curl -fL "${GO_URL}" -o "${GO_ARCHIVE}"

verify_sha256 "${GO_ARCHIVE}" "${GO_SHA256}"

rm -rf "${1}/go"

tar -xzf "${GO_ARCHIVE}" -C "${1}"

ln -sf "../go/bin/go" "${1}/bin/go"
ln -sf "../go/bin/gofmt" "${1}/bin/gofmt"

"${1}/bin/go" version
