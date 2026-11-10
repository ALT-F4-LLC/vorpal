#!/usr/bin/env bash
#
# stub — body of a Docket gate with no real check wired yet.
#
# Backs `sdet-abuse` and `vuln-scan`. It exits 0 without checking, so their
# trust entries carry `stub(no-real-check: ...)` to mark the passes hollow.
# Replace a target's call to this script with a real check to make it real.
#
# usage: stub.sh <make-target>

set -euo pipefail

if [[ "$#" -ne 1 ]]; then
  echo "usage: $0 <make-target>" >&2
  exit 2
fi

echo "STUB: 'make $1' performed no check (see 'Docket gates' in makefile)"
