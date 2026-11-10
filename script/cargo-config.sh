#!/usr/bin/env bash
#
# cargo-config — point cargo at the vendored sources in <vendor-dir>.
#
# Appends to <cargo-dir>/config.toml, so run it once on a fresh directory; the
# `.cargo` make target runs it only while <cargo-dir> is absent.
#
# usage: cargo-config.sh <cargo-dir> <vendor-dir>

set -euo pipefail

if [[ "$#" -ne 2 ]]; then
  echo "usage: $0 <cargo-dir> <vendor-dir>" >&2
  exit 2
fi

cargo_dir="$1"
vendor_dir="$2"

mkdir -p "$cargo_dir"
{
  echo '[source.crates-io]'
  echo 'replace-with = "vendored-sources"'
  echo '[source.vendored-sources]'
  echo "directory = \"$vendor_dir\""
} >> "$cargo_dir/config.toml"
