#!/usr/bin/env bash
set -euo pipefail

export PATH="${1}/bin:${PATH}"

if [[ -f "${1}/bin/lima" ]]; then
    "${1}/bin/lima" --version
    exit 0
fi

VERSION=$(curl -fsSL https://api.github.com/repos/lima-vm/lima/releases/latest | jq -r .tag_name)

curl -fsSL "https://github.com/lima-vm/lima/releases/download/${VERSION}/lima-${VERSION:1}-$(uname -s)-$(uname -m).tar.gz" | tar Cxzvm "${1}"
