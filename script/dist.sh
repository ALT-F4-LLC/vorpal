#!/usr/bin/env bash
#
# dist — package the built vorpal binary as a release tarball.
#
# usage: dist.sh <dist-dir> <work-dir> <arch> <os> <target>

set -euo pipefail

if [[ "$#" -ne 5 ]]; then
  echo "usage: $0 <dist-dir> <work-dir> <arch> <os> <target>" >&2
  exit 2
fi

dist_dir="$1"
work_dir="$2"
arch="$3"
os="$4"
target="$5"

mkdir -p "$dist_dir"
tar -czf "$dist_dir/vorpal-$arch-$os.tar.gz" \
  -C "$work_dir/target/$target" \
  vorpal
