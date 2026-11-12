#!/usr/bin/env bash
#
# ac-commands — run `build` and `test` and report each result.
#
# Pre-gate on `verify`. Its output gives the verifier recorded command results
# to judge "the tests pass" criteria against; without them it must answer
# `unverifiable`, which parks the step. A failing pre-gate does not refuse the
# claim: the engine passes the failure along as data.
#
# The exit status is the worst any check recorded, not the sum, because summed
# exits wrap at 256 back to 0.
#
# Limit: it cannot run the fenced AC commands harvested per issue. A gate gets
# no run, issue or step identity, so it cannot find them.
#
# No `set -e`: a failing check is recorded and the loop continues. The make
# binary is $MAKE when set, else `make`.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "ac-commands FAILED: $script_dir is not inside a git repository." >&2
  exit 1
fi
cd "$repo_root"

make_bin="${MAKE:-make}"

echo "=== ac-commands: recorded results for the verifier ==="
echo "commit: $(git rev-parse --short HEAD)"
echo "tree:   $(git status --porcelain | wc -l | tr -d ' ') uncommitted path(s)"

worst=0
log="$(mktemp)" || exit 1
trap 'rm -f "$log"' EXIT

for target in build test; do
  echo "--- $target: make $target ---"
  status=0
  "$make_bin" "$target" >"$log" 2>&1 || status=$?
  tail -20 "$log"
  echo "[$target] exit $status"
  if [ "$status" -gt "$worst" ]; then worst=$status; fi
  echo
done

echo "=== end ac-commands ==="
exit "$worst"
