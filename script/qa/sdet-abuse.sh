#!/usr/bin/env bash
#
# sdet-abuse — abuse-case tests exist and pass.
#
# Gate on the `implement` and `fix` steps of the security track, whose threat
# model names the abuse cases a security-load-bearing change must execute.
#
# Convention: a Rust test whose path contains `abuse` is an abuse-case test
# (`cargo test abuse` selects by substring of the full path, so a module named
# `abuse` or a function named `rejects_oversize_secret_abuse` both count). The
# gate runs only those tests: `tests` is its own gate, and running the whole
# suite here would surface one failure as two.
#
# Fails closed on each way an abuse case goes unchecked:
#   - zero tests match: an empty run would otherwise read as a pass;
#   - a matched test is ignored: an unrun abuse case is not a pass;
#   - the run executed fewer tests than the listing promised;
#   - an abuse-named test exists in Go, Python or TypeScript, which this gate
#     does not run. Rename it, or extend this gate to run that toolchain.
#
# What it cannot do: tie tests to the threat model's abuse-case ids. A gate
# gets no run, issue or step identity, so it cannot read the threat model. A
# reviewer still confirms the cases match. A test behind `#[cfg(target_os)]`
# does not exist on the other OS, so the count is per host and the zero rule
# applies there too.
#
# Cargo flags come from $CARGO_FLAGS (set by make), so the run reuses the
# `tests` build. No `set -e`: each check records its failure and the script
# reports every reason at once.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "sdet-abuse FAILED: $script_dir is not inside a git repository." >&2
  exit 1
fi
cd "$repo_root"

work="$(mktemp -d "${TMPDIR:-/tmp}/sdet-abuse.XXXXXX")" || exit 1
trap 'rm -rf "$work"' EXIT

reasons=()

echo "=== sdet-abuse: abuse-case tests (cargo test abuse) ==="

# shellcheck disable=SC2086 # CARGO_FLAGS is a flag list by design
if ! cargo test ${CARGO_FLAGS:-} abuse -- --list >"$work/list.out" 2>"$work/list.err"; then
  echo "sdet-abuse FAILED: could not list the abuse-case tests." >&2
  tail -20 "$work/list.err" >&2
  exit 1
fi
listed="$(grep -c ': test$' "$work/list.out" || true)"
echo "abuse-case tests listed: $listed"

if [ "$listed" -eq 0 ]; then
  reasons+=("no Rust test path contains 'abuse'; name each abuse-case test for the case it exercises (for example a function ending in _abuse or a module named abuse)")
else
  # shellcheck disable=SC2086
  cargo test ${CARGO_FLAGS:-} abuse >"$work/run.out" 2>&1
  status=$?
  tail -40 "$work/run.out"
  passed="$(awk '/^test result:/ { for (i = 1; i <= NF; i++) if ($i == "passed;") p += $(i - 1) } END { print p + 0 }' "$work/run.out")"
  ignored="$(awk '/^test result:/ { for (i = 1; i <= NF; i++) if ($i == "ignored;") n += $(i - 1) } END { print n + 0 }' "$work/run.out")"
  echo "abuse-case tests passed: $passed, ignored: $ignored"
  [ "$status" -eq 0 ] || reasons+=("cargo test abuse exited $status; a matched abuse-case test failed")
  [ "$ignored" -eq 0 ] || reasons+=("$ignored matched abuse-case test(s) are #[ignore]d; an unrun abuse case is not a pass")
  [ "$passed" -gt 0 ] || reasons+=("no abuse-case test passed")
  [ "$passed" -eq "$listed" ] || reasons+=("cargo listed $listed abuse-case test(s) but $passed passed")
fi

# Abuse-named tests in toolchains this gate does not run.
: >"$work/other.txt"
git ls-files -z -- '*_test.go' | xargs -0 grep -inE 'func Test[A-Za-z0-9_]*abuse' 2>/dev/null >>"$work/other.txt" || true
git ls-files -z -- '*test_*.py' '*_test.py' | xargs -0 grep -inE '(def test_[A-Za-z0-9_]*abuse|class Test[A-Za-z0-9_]*abuse)' 2>/dev/null >>"$work/other.txt" || true
git ls-files -z -- '*.test.ts' '*.spec.ts' | xargs -0 grep -inE "(it|test|describe)\(\s*['\"\`][^'\"\`]*abuse" 2>/dev/null >>"$work/other.txt" || true
if [ -s "$work/other.txt" ]; then
  echo "abuse-named tests this gate does not run:" >&2
  sed 's/^/  /' "$work/other.txt" >&2
  reasons+=("abuse-named test(s) exist in Go, Python or TypeScript, which this gate does not run")
fi

if [ "${#reasons[@]}" -gt 0 ]; then
  echo >&2
  echo "sdet-abuse FAILED:" >&2
  for r in "${reasons[@]}"; do
    echo "  - $r" >&2
  done
  exit 1
fi

echo "sdet-abuse: ok"
exit 0
