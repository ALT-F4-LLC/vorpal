#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
PROTOC_SHA256=""
PROTOC_SYSTEM=""
PROTOC_VERSION="34.0"

if [[ -f "${1}/bin/protoc" ]]; then
    "${1}/bin/protoc" --version
    exit 0
fi

if [[ "${OS}" == "darwin" ]]; then
    PROTOC_SYSTEM="osx"
elif [[ "${OS}" == "linux" ]]; then
    PROTOC_SYSTEM="linux"
else
    echo "Unsupported OS: ${OS}"
    exit 1
fi

if [[ "${ARCH}" == "x86_64" ]]; then
    PROTOC_SYSTEM="${PROTOC_SYSTEM}-x86_64"
elif [[ "${ARCH}" == "arm64" || "${ARCH}" == "aarch64" ]]; then
    PROTOC_SYSTEM="${PROTOC_SYSTEM}-aarch_64"
else
    echo "Unsupported ARCH: ${ARCH}"
    exit 1
fi

if [[ "$PROTOC_SYSTEM" == "" ]]; then
    echo "PROTOC_SYSTEM is empty"
    exit 1
fi

case "${PROTOC_VERSION}-${PROTOC_SYSTEM}" in
  "34.0-osx-aarch_64")
    PROTOC_SHA256="3ef35187a3c8aed81ee57e792227e483e558fa56c93fce525e569bff55794c1a"
    ;;
  "34.0-osx-x86_64")
    PROTOC_SHA256="d58fcd413a9ed458283d54023e409fd5cf767da4ed225d1ffaffd83cf2764f53"
    ;;
  "34.0-linux-aarch_64")
    PROTOC_SHA256="f0b8aad28be5ea6150c082f96ac57e028154afb9ee29f4ce092b5a39df8ae6c8"
    ;;
  "34.0-linux-x86_64")
    PROTOC_SHA256="e9a91b6fcfe4177ec2cd35fc8f15c1e811fa0ecdef9372755cd6d3513d5faaab"
    ;;
esac

PROTOC_TMPDIR="$(mktemp -d)"
PROTOC_ARCHIVE="${PROTOC_TMPDIR}/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"

trap 'rm -rf "${PROTOC_TMPDIR}"' EXIT

curl -fL \
    "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip" \
    -o "${PROTOC_ARCHIVE}"

verify_sha256 "${PROTOC_ARCHIVE}" "${PROTOC_SHA256}"

unzip "${PROTOC_ARCHIVE}" -d "${1}"
