#!/usr/bin/env bash
#
# clean — remove build output and the cargo config, dist and vendor dirs.
#
# usage: clean.sh <cargo-dir> <dist-dir> <vendor-dir>

set -euo pipefail

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 <cargo-dir> <dist-dir> <vendor-dir>" >&2
  exit 2
fi

cargo clean
rm -rf "$1"
rm -rf "$2"
rm -rf "$3"
