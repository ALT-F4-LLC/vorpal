#!/usr/bin/env bash
#
# platform — print the lowercased machine architecture or operating system.
# Prints `aarch64` for `arm64` so macOS and Linux agree.
#
# usage: platform.sh arch|os

set -euo pipefail

case "${1:-}" in
  arch)
    uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/'
    ;;
  os)
    uname -s | tr '[:upper:]' '[:lower:]'
    ;;
  *)
    echo "usage: $0 arch|os" >&2
    exit 2
    ;;
esac
