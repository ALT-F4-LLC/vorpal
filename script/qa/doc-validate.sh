#!/usr/bin/env bash
#
# doc-validate — check that changed docs carry this repo's header shape.
#
# Gate on the implement and fix steps of every code track:
#
#   website/src/content/docs/**  YAML frontmatter that opens on line 1, closes
#                                within FRONTMATTER_MAX_LINES, and has
#                                non-empty `title:` and `description:`.
#   */SKILL.md                   the same frontmatter block, no key check.
#   any other *.md               opens with a `# ` title.
#
# Scope is changed docs only (committed since DOCKET_GATE_BASE, staged,
# unstaged, untracked); an older malformed doc would make the gate unclearable.
#
# Exit 0 on pass, 1 on a wrong shape, 2 when run as the gate without a usable
# DOCKET_GATE_BASE (fails closed: the tree is empty once the step commits).

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

# Succeeds when the file opens with '---' and closes it within the first
# FRONTMATTER_MAX_LINES lines. The bound stops a later '---' (a thematic break)
# from passing as the closing fence when the real one was deleted.
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
# `while read`, not `for`: word splitting would turn a name with a space into
# two non-files that the -f guard then skips.
while IFS= read -r f; do
  [ -n "$f" ] || continue
  # A deleted doc has nothing left to validate.
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
      # *.mdx outside the website tree has no convention.
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
