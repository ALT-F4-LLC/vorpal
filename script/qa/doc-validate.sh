#!/usr/bin/env bash
#
# doc-validate — a changed doc carries the header shape this repo's docs carry.
#
# Wired as the `doc-validate` gate that the docket workflows run on the
# implement and fix steps of every code track. Adapted from the docket
# corpus's doc-validate, anchored to THIS repository's conventions:
#
#   website/src/content/docs/**  Starlight pages: a YAML frontmatter block
#                                that opens on line 1, closes within
#                                FRONTMATTER_MAX_LINES lines, and names both
#                                `title:` and `description:` (Starlight
#                                refuses a page without a title, and every
#                                page in the tree carries a description).
#   */SKILL.md                   a YAML frontmatter block that opens on line 1
#                                and closes within FRONTMATTER_MAX_LINES lines.
#   any other *.md               opens with a `# ` title.
#
# Scope is CHANGED docs only: committed since DOCKET_GATE_BASE, staged,
# unstaged, and untracked. A malformed doc that predates the step is not the
# step's doing, and failing on it would make the gate unclearable.
#
# Exit 0 on pass, 1 when a changed doc has the wrong shape, 2 when this runs as
# the gate without a usable DOCKET_GATE_BASE (fail closed: the step commits its
# work before the record-time rerun, so the tree alone is empty by then).

set -euo pipefail

FRONTMATTER_MAX_LINES=40

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "doc-validate FAILED: $script_dir is not inside a git repository." >&2
  exit 1
fi
cd "$repo_root"

committed_range=""
base="${DOCKET_GATE_BASE:-}"
if [ -n "$base" ] && git rev-parse --verify --quiet "${base}^{commit}" >/dev/null; then
  committed_range="${base}..HEAD"
elif [ "${DOCKET_GATE:-}" = "doc-validate" ]; then
  echo "doc-validate FAILED: DOCKET_GATE_BASE is unset or does not resolve to a commit ('${base}'); cannot select committed docs." >&2
  exit 2
fi

pathspec=('*.md' '*.mdx')
changed=$( { if [ -n "$committed_range" ]; then
               git diff --name-only "$committed_range" -- "${pathspec[@]}"
             fi
             git diff --cached --name-only -- "${pathspec[@]}"
             git diff --name-only -- "${pathspec[@]}"
             git ls-files --others --exclude-standard -- "${pathspec[@]}"
           } | sort -u )

if [ -z "$changed" ]; then
  echo "doc-validate: no docs changed"
  exit 0
fi

# Succeeds when the file opens with '---' and a closing '---' appears within
# the first FRONTMATTER_MAX_LINES lines. The bound matters: an unbounded scan
# treats any later '---' (a thematic break) as the terminator, so a page whose
# real closing fence was deleted would still read as terminated.
frontmatter_ok() {
  head -1 "$1" | grep -q '^---$' &&
    awk -v max="$FRONTMATTER_MAX_LINES" \
      'NR > 1 && NR <= max && $0 == "---" { found = 1; exit }
       NR > max { exit }
       END { exit !found }' "$1"
}

# Prints the frontmatter block's body (between the fences).
frontmatter_body() {
  awk -v max="$FRONTMATTER_MAX_LINES" \
    'NR == 1 { next } NR > max || $0 == "---" { exit } { print }' "$1"
}

failed=0
# `while read`, never `for f in $changed`: word splitting turns a name with a
# space into two non-files, and the -f guard would then skip both.
while IFS= read -r f; do
  [ -n "$f" ] || continue
  # A deleted doc appears in the diff but has nothing left to validate.
  [ -f "$f" ] || continue
  echo "doc-validate: checking $f"

  case "$f" in
    website/src/content/docs/*)
      if ! frontmatter_ok "$f"; then
        echo "doc-validate: $f does not open with a terminated '---' frontmatter block" >&2
        failed=1
        continue
      fi
      body="$(frontmatter_body "$f")"
      for key in title description; do
        if ! printf '%s\n' "$body" | grep -q "^${key}:[[:space:]]*[^[:space:]]"; then
          echo "doc-validate: $f frontmatter has no non-empty '${key}:'" >&2
          failed=1
        fi
      done
      ;;
    */SKILL.md|SKILL.md)
      if ! frontmatter_ok "$f"; then
        echo "doc-validate: $f does not open with a terminated '---' frontmatter block" >&2
        failed=1
      fi
      ;;
    *.md)
      if ! head -1 "$f" | grep -q '^# '; then
        echo "doc-validate: $f does not open with a '# ' title" >&2
        failed=1
      fi
      ;;
    *)
      # Any other *.mdx outside the website tree has no convention here.
      ;;
  esac
done <<< "$changed"

if [ "$failed" -ne 0 ]; then
  echo "" >&2
  echo "doc-validate FAILED. Existing pages under website/src/content/docs/ and" >&2
  echo "README.md show the expected shapes." >&2
  exit 1
fi

echo "doc-validate: ok"
