#!/usr/bin/env bash
#
# diff-scope — refuse a change whose footprint exceeds what its track promised.
#
# Vendored from the docket corpus: dotfiles.vorpal.git .docket/bin/diff-scope.
# Gate on the implement and fix steps of the docs-only, trivial and small
# tracks. Those bind on a label alone and nothing else reads the diff, so
# without this a "trivial" label could carry a 400-line change past every
# judge.
#
# Footprint: the committed range DOCKET_GATE_BASE..HEAD, the working tree
# against HEAD (staged and unstaged), and untracked files. A rename counts as a
# delete plus an add, so moving a file off its path counts that path too.
#
# Rules, per track:
#   docs-only  every touched path is a document (see is_doc);
#   trivial    at most one path, no added or deleted file, at most
#              TRIVIAL_MAX_LINES changed lines;
#   small      at most two paths, in one directory, no added file.
# Every track also fails on a path matching a glob in the optional
# .docket/scope-control-paths (one per line, `#` comments), so a change to a
# control class never rides a light track.
#
# Exit 0 on pass; 1 on refusal, listing every reason; 2 on usage, a non-git
# cwd, a missing or unresolvable base, or a failing git read (fails closed).

set -euo pipefail

TRIVIAL_MAX_LINES="${DIFF_SCOPE_TRIVIAL_MAX_LINES:-10}"
CONTROL_FILE="${DIFF_SCOPE_CONTROL_FILE:-.docket/scope-control-paths}"

usage() {
    echo "usage: diff-scope docs-only|trivial|small" >&2
    exit 2
}

[ "$#" -eq 1 ] || usage
TRACK="$1"
case "$TRACK" in
    docs-only|trivial|small) ;;
    *) usage ;;
esac

git rev-parse --show-toplevel >/dev/null 2>&1 || {
    echo "diff-scope: not inside a git work tree; refusing (cannot read the footprint)" >&2
    exit 2
}

[ -n "${DOCKET_GATE_BASE:-}" ] || {
    echo "diff-scope: DOCKET_GATE_BASE is unset or empty; refusing (cannot read the committed footprint)" >&2
    exit 2
}
base=$(git rev-parse --verify --quiet --end-of-options "${DOCKET_GATE_BASE}^{commit}") || {
    echo "diff-scope: DOCKET_GATE_BASE '${DOCKET_GATE_BASE}' does not resolve to a commit; refusing" >&2
    exit 2
}
head=$(git rev-parse --verify --quiet 'HEAD^{commit}') || {
    echo "diff-scope: HEAD does not resolve to a commit; refusing" >&2
    exit 2
}

# Each git read lands in a file and is checked before parsing, so a failed read
# cannot pass as an empty footprint. Files, not variables, because -z output
# carries NULs.
scratch=$(mktemp -d "${TMPDIR:-/tmp}/diff-scope.XXXXXX") || {
    echo "diff-scope: cannot create a scratch directory; refusing" >&2
    exit 2
}
trap 'rm -rf "$scratch"' EXIT

read_git() { # <out-file> <what> <git-args...>
    local out="$1" what="$2"
    shift 2
    git "$@" > "$out" || {
        echo "diff-scope: cannot read the $what (git $1 failed); refusing" >&2
        exit 2
    }
}

read_git "$scratch/committed" "committed footprint" \
    diff --no-ext-diff --no-textconv --no-renames -z --name-status "$base" "$head"
read_git "$scratch/status" "working-tree footprint" \
    status --no-renames --porcelain=v1 -z --untracked-files=all
read_git "$scratch/committed-lines" "committed line count" \
    diff --no-ext-diff --no-textconv --no-renames --numstat "$base" "$head"
read_git "$scratch/worktree-lines" "working-tree line count" \
    diff --no-ext-diff --no-textconv --no-renames --numstat HEAD

is_doc() { # <path>
    local p="$1" base
    base=$(basename "$p")
    case "$p" in
        docs/*|doc/*|documentation/*|*/docs/*|*/doc/*) return 0 ;;
    esac
    case "$base" in
        *.md|*.mdx|*.markdown|*.txt|*.rst|*.adoc) return 0 ;;
        README*|LICENSE*|LICENCE*|CHANGELOG*|CONTRIBUTING*|CODEOWNERS|NOTICE*|AUTHORS*) return 0 ;;
    esac
    return 1
}

# ---- collect the footprint ------------------------------------------------
paths=()     # every touched path, deduplicated
added=()      # new files: committed or index A, or untracked
deleted=()    # removed files

# Committed range first; `--name-status -z` emits status\0path\0. No path is
# skipped: a committed file is the step's change wherever it lives.
while IFS= read -r -d '' status; do
    IFS= read -r -d '' path || break
    paths+=("$path")
    case "$status" in
        A*) added+=("$path") ;;
        D*) deleted+=("$path") ;;
    esac
done < "$scratch/committed"

# `git status --porcelain=v1 -z` is stable across versions and locales: one
# `XY path` record per path.
while IFS= read -r -d '' rec; do
    xy=${rec:0:2}
    path=${rec:3}
    [ -n "$path" ] || continue
    # A sandboxed session mounts `.claude/` and `.mcp.json` into its cwd, and
    # git reports them untracked. Skip them only while untracked: a modified
    # tracked file under `.claude/` still counts.
    if [ "$xy" = '??' ]; then
        case "/$path" in
            */.claude/*|*/.claude) continue ;;
            */.mcp.json) continue ;;
        esac
    fi
    paths+=("$path")
    case "$xy" in
        '??'|A*|*A) added+=("$path") ;;
        D*|*D) deleted+=("$path") ;;
    esac
done < "$scratch/status"

# Dedupe, keeping order (bash 3.2 compatible).
if [ "${#paths[@]}" -gt 0 ]; then
    deduped=()
    while IFS= read -r p; do
        deduped+=("$p")
    done < <(printf '%s\n' "${paths[@]}" | awk '!seen[$0]++')
    paths=("${deduped[@]}")
fi
nfiles=${#paths[@]}

# Changed lines: the committed range, tracked changes against HEAD, and every
# line of an untracked file. A path edited in both places counts in both.
lines=0
while IFS=$'\t' read -r ins del _p; do
    [ -n "${ins:-}" ] || continue
    [ "$ins" = "-" ] && ins=0   # binary
    [ "$del" = "-" ] && del=0
    lines=$((lines + ins + del))
done < <(cat "$scratch/committed-lines" "$scratch/worktree-lines")
for p in "${added[@]:-}"; do
    [ -n "$p" ] || continue
    if git ls-files --error-unmatch -- "$p" >/dev/null 2>&1; then continue; fi  # staged adds are in numstat already
    [ -f "$p" ] && lines=$((lines + $(wc -l < "$p")))
done

# ---- judge -----------------------------------------------------------------
reasons=()

if [ -f "$CONTROL_FILE" ]; then
    while IFS= read -r glob; do
        glob=${glob%%#*}; glob=${glob//[[:space:]]/}
        [ -n "$glob" ] || continue
        for p in "${paths[@]:-}"; do
            [ -n "$p" ] || continue
            # shellcheck disable=SC2254
            case "$p" in
                $glob) reasons+=("control-class path touched (matches '$glob' in $CONTROL_FILE): $p; a change under a control class never rides a light track") ;;
            esac
        done
    done < "$CONTROL_FILE"
fi

case "$TRACK" in
    docs-only)
        for p in "${paths[@]:-}"; do
            [ -n "$p" ] || continue
            is_doc "$p" || reasons+=("not a document: $p")
        done
        ;;
    trivial)
        [ "$nfiles" -le 1 ] || reasons+=("$nfiles paths touched; trivial allows one")
        [ "${#added[@]}" -eq 0 ] || reasons+=("added file(s): ${added[*]}; trivial adds nothing")
        [ "${#deleted[@]}" -eq 0 ] || reasons+=("deleted file(s): ${deleted[*]}; trivial deletes nothing")
        [ "$lines" -le "$TRIVIAL_MAX_LINES" ] || reasons+=("$lines changed lines; trivial allows $TRIVIAL_MAX_LINES")
        ;;
    small)
        [ "$nfiles" -le 2 ] || reasons+=("$nfiles paths touched; small allows two")
        [ "${#added[@]}" -eq 0 ] || reasons+=("added file(s): ${added[*]}; small adds no file")
        if [ "$nfiles" -eq 2 ]; then
            d1=$(dirname "${paths[0]}"); d2=$(dirname "${paths[1]}")
            [ "$d1" = "$d2" ] || reasons+=("paths span two directories ($d1, $d2); small stays in one")
        fi
        ;;
esac

if [ "${#reasons[@]}" -gt 0 ]; then
    echo "diff-scope: REFUSED for track '$TRACK' ($nfiles paths, $lines changed lines):"
    printf '  - %s\n' "${reasons[@]}"
    echo "  The change outgrew its size label. Either narrow the change to what the label promises, or ask the operator to relabel the issue for a track whose gates match the footprint (standard-change runs judges, build, tests and scans)."
    exit 1
fi

echo "diff-scope: ok ($TRACK: $nfiles paths, $lines changed lines)"
