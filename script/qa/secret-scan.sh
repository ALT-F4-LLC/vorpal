#!/usr/bin/env bash
#
# secret-scan — refuse a change that adds a credential.
#
# Scans only what this step added: a secret in an older commit would make the
# gate unclearable for every later step. Four sources, since a change hides in
# any of them:
#
#   staged     absent from both the working-tree diff and the untracked list.
#   unstaged   the ordinary in-progress edit.
#   untracked  where a new file's credential lands; `git diff` cannot see it.
#   HEAD       load-bearing: the engine re-runs gates after the hand-back
#              commit against a clean tree, so the other three are empty by
#              then and HEAD's patch is the change under review.
#
# Limit: HEAD is one commit, so earlier commits of a multi-commit hand-back
# were scanned only on earlier runs, while still staged or unstaged.
#
# Fails closed: a git that errors would leave an empty input that reads as
# "clean", so every git call's exit status is checked. That is why it writes to
# files and then greps (a pipeline hides the first command's failure), and why
# there is no `set -e`.
#
# Never prints a matching line, which would leak the secret into the gate's
# verdict and transcripts; the count is enough. Untracked names reach `sh` as
# arguments, never in its command string, so a hostile filename cannot run.
#
# Patterns are high-signal only: one that fires on ordinary code trains people
# to route around the gate. `github_pat_` is separate from `gh[pousr]_`
# because fine-grained tokens use a different prefix.

set -u

SECRET_PATTERNS='AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,}|xox[abprs]-[0-9A-Za-z-]{10,}|sk-ant-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}|-----BEGIN [A-Z ]*PRIVATE KEY-----'

collect_failed() {
  echo "secret-scan FAILED: could not collect the change set; nothing was scanned." >&2
  exit 1
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "secret-scan FAILED: not a git repository, so nothing can be scanned." >&2
  exit 1
fi
cd "$repo_root"

raw="$(mktemp)" || exit 1
scanned="$(mktemp)" || exit 1
list="$(mktemp)" || exit 1
trap 'rm -f "$raw" "$scanned" "$list"' EXIT

git diff --cached --unified=0 -- . >"$raw" || collect_failed
git diff --unified=0 -- . >>"$raw" || collect_failed
git show --unified=0 --format= --diff-merges=first-parent HEAD -- . >>"$raw" || collect_failed
/usr/bin/grep '^+' "$raw" | /usr/bin/grep -v '^+++' >"$scanned" || true

git ls-files --others --exclude-standard -z >"$list" || collect_failed
xargs -0 sh -c 'for f; do if test -f "./$f"; then cat "./$f" || exit 1; fi; done' _ <"$list" >>"$scanned" || collect_failed

if [ ! -s "$scanned" ]; then
  echo "secret-scan: no added lines to scan"
  exit 0
fi

hits="$(/usr/bin/grep -cE "$SECRET_PATTERNS" "$scanned" || true)"
if [ "$hits" -gt 0 ]; then
  echo "secret-scan FAILED: $hits added line(s) match a credential pattern." >&2
  echo "Remove it and re-run. If it is a fixture, make it unmistakably fake." >&2
  exit 1
fi

echo "secret-scan: clean"
