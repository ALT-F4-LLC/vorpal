#!/usr/bin/env bash
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib/verify.sh"

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
TERRAFORM_ARCH=""
TERRAFORM_OS=""
TERRAFORM_SHA256=""
TERRAFORM_VERSION="1.14.6"

case "${OS}" in
  darwin|linux)
    TERRAFORM_OS="${OS}"
    ;;
  *)
    echo "Unsupported OS: ${OS}" >&2
    exit 1
    ;;
esac

case "${ARCH}" in
  x86_64|amd64)
    TERRAFORM_ARCH="amd64"
    ;;
  arm64|aarch64)
    TERRAFORM_ARCH="arm64"
    ;;
  *)
    echo "Unsupported ARCH: ${ARCH}" >&2
    exit 1
    ;;
esac

mkdir -p "${1}/bin"

if [[ -x "${1}/bin/terraform" ]]; then
  "${1}/bin/terraform" version || true
  exit 0
fi

case "${TERRAFORM_VERSION}_${TERRAFORM_OS}_${TERRAFORM_ARCH}" in
  "1.14.6_darwin_amd64")
    TERRAFORM_SHA256="ae13b4d204b00a0742f13a3de78a78994918f31333b1537db682cd0a7085dac0"
    ;;
  "1.14.6_darwin_arm64")
    TERRAFORM_SHA256="5d91a8d6877e792de00be8db2324a2561edeb312ec2ff141b877131b82622c76"
    ;;
  "1.14.6_linux_amd64")
    TERRAFORM_SHA256="364c6ee08b0cb8fcbb28a115aacb2aa48e88abc56c149170bd65c2f75d98ea8d"
    ;;
  "1.14.6_linux_arm64")
    TERRAFORM_SHA256="190037f64695556ac75965c00da5d85b3663f38553d909e9a51c4490cba4b6c1"
    ;;
esac

TERRAFORM_TMPDIR="$(mktemp -d)"
TERRAFORM_ARCHIVE="${TERRAFORM_TMPDIR}/terraform_${TERRAFORM_VERSION}_${TERRAFORM_OS}_${TERRAFORM_ARCH}.zip"
TERRAFORM_URL="https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_${TERRAFORM_OS}_${TERRAFORM_ARCH}.zip"

trap 'rm -rf "${TERRAFORM_TMPDIR}"' EXIT

echo "Downloading Terraform ${TERRAFORM_VERSION} (${TERRAFORM_OS}/${TERRAFORM_ARCH})..."

curl -fL "${TERRAFORM_URL}" -o "${TERRAFORM_ARCHIVE}"

verify_sha256 "${TERRAFORM_ARCHIVE}" "${TERRAFORM_SHA256}"

unzip -q "${TERRAFORM_ARCHIVE}" -d "${TERRAFORM_TMPDIR}"

install -m 0755 "${TERRAFORM_TMPDIR}/terraform" "${1}/bin/terraform"

"${1}/bin/terraform" version
