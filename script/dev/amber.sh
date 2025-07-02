#!/usr/bin/env bash
set -euo pipefail

AMBER_URL="https://github.com/amber-lang/amber/releases/download/0.4.0-alpha/amber-aarch64-apple-darwin.tar.xz"
TEMP_FILE="/tmp/amber-aarch64-apple-darwin.tar.xz"

export PATH="${1}/bin:${PATH}"

if [[ -f "${1}/bin/amber" ]]; then
    "${1}/bin/amber" --version
    exit 0
fi

curl -L -o "${TEMP_FILE}" "${AMBER_URL}"

tar -xf "${TEMP_FILE}" -C "${1}/bin"

rm "${TEMP_FILE}"

# chmod +x "${1}/bin/amber"
