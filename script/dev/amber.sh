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

# Move amber binary from extracted directory to bin root
mv "${1}/bin/amber-aarch64-apple-darwin/amber" "${1}/bin/amber"

# Clean up extracted directory
rm -rf "${1}/bin/amber-aarch64-apple-darwin"

rm "${TEMP_FILE}"

chmod +x "${1}/bin/amber"
