#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS="$(uname | tr '[:upper:]' '[:lower:]')"
TERRAFORM_ARCH=""
TERRAFORM_OS=""
TERRAFORM_VERSION="1.13.3"

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

TERRAFORM_ARCHIVE="/tmp/terraform_${TERRAFORM_VERSION}_${TERRAFORM_OS}_${TERRAFORM_ARCH}.zip"
TERRAFORM_URL="https://releases.hashicorp.com/terraform/${TERRAFORM_VERSION}/terraform_${TERRAFORM_VERSION}_${TERRAFORM_OS}_${TERRAFORM_ARCH}.zip"

echo "Downloading Terraform ${TERRAFORM_VERSION} (${TERRAFORM_OS}/${TERRAFORM_ARCH})..."

curl -fL "${TERRAFORM_URL}" -o "${TERRAFORM_ARCHIVE}"

TMPDIR="$(mktemp -d)"

trap 'rm -rf "${TMPDIR}" "${TERRAFORM_ARCHIVE}"' EXIT

unzip -q "${TERRAFORM_ARCHIVE}" -d "${TMPDIR}"

install -m 0755 "${TMPDIR}/terraform" "${1}/bin/terraform"

"${1}/bin/terraform" version
