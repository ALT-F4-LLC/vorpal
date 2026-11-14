#!/bin/bash
set -euo pipefail

# =============================================================================
# Vorpal Source Tool
# =============================================================================
# Downloads, verifies, and stages the upstream sources behind every SDK
# artifact for upload to the mirror at https://sdk.vorpal.build/source/.
#
# Usage:
#   script/source.sh [lock]
#       Download every source in Vorpal.lock to ./source/<platform>/<file>.
#   script/source.sh check [FILTER...] [--dir DIR]
#       Dry-run report: artifact, kind, current, target, status, authority
#       URL. Writes only DIR/plan.json. Status is current, bump, held (target
#       older than current), or blocked (no target, or a declared system or
#       tag missing upstream).
#   script/source.sh stage [--dir DIR] [--include-current]
#       For every "bump" in DIR/plan.json: download from upstream, verify with
#       the declared authority, and write DIR/objects/<key> plus
#       DIR/manifest.json. Fails closed; exit 4 means a keyring is unpinned.
#   script/source.sh mirror-check [--dir DIR] [--present]
#       Compare each staged key with the mirror: absent, same, or conflict.
#       A key that exists with another SHA-256 stops with exit code 3.
#       --present (after upload) only asks whether each key exists, without
#       downloading it; any absent key exits 6. vorpal prepare then digests
#       the mirror bytes, and the supply-chain test compares those digests
#       with the staged ones.
#   script/source.sh upload-cmd --bucket BUCKET [--dir DIR]
#       Print the no-overwrite upload command for every absent key.
#   script/source.sh provenance [--dir DIR] [--out FILE]
#       Merge one record per staged object into provenance.json.
#   script/source.sh pins [FILTER...]
#       Check that every SDK pin carries its registry version.
#   script/source.sh set-version --name NAME --to VERSION [--release R]
#       Rewrite NAME's pins and registry version. linux-vorpal edits only the
#       Rust file; run sdk/codegen for Go, TypeScript, and Python.
#   script/source.sh digest-fill --print | --apply FILE [--dir DIR]
#       On Linux, fill inline digests the macOS stage deferred (paths that
#       differ only in case).
#   script/source.sh digest FILE
#       Print the inline digest Vorpal computes for FILE as a source.
#   script/source.sh book-diff [--to VERSION] [--dir DIR]
#       Diff the commands of every mapped LFS/BLFS section between the tracked
#       book and VERSION (default: stable), beside the stage blocks that
#       implement them; report new, removed, and reordered sections. Stages
#       snapshots and DIR/book/plan.json.
#   script/source.sh book-diff --accept [--dir DIR]
#       Install the staged snapshots and move the tracked books to the plan's
#       target. Changed sections, and on a move every unaudited one, become
#       unreconciled.
#   script/source.sh book-diff --mark SECTION --disposition edited|unchanged|deviation [--reason R]
#       Record SECTION as reviewed against its stage blocks and refresh their
#       hashes; "edited" requires a changed block. --mark local:FILE:HEADING
#       refreshes a local block.
#   script/source.sh book-diff --map FILE:HEADING=SECTION[,SECTION]|local:REASON
#       Map a new or renamed stage block (repeatable; combines with --mark).
#       Add --reorder to accept stage blocks that moved.
#   script/source.sh book-diff --skip SECTION --reason R
#       Record a book section the stage scripts do not implement.
#   script/source.sh book-diff --unmap SECTION | --show SECTION
#       Drop a mapped section, or print its snapshot beside its stage blocks.
#   script/source.sh book-diff --check [--offline]
#       Fail unless stage blocks, ledger, snapshots, and (online) the tracked
#       book agree.
#
# Filters (combine freely; an entry must match all of them):
#   --kind KIND        tool | rust | linux-vorpal (repeatable)
#   --not-kind KIND    exclude a kind (repeatable), e.g. --not-kind linux-vorpal
#   --group GROUP      go | rust | typescript | python | oci | vcs | protobuf | system
#   --name NAME        registry entry name (repeatable)
#   --not-name NAME    exclude a registry entry (repeatable)
#   --target-version V override the target for one pin-sharing group, every
#                      member named with --name; never Rust; book-tracked
#                      sources also need --advisory LFS/BLFS-advisory-URL
#
# DIR defaults to .source-stage at the repository root. Reads
# script/source/registry.json, script/source/ledger.json, and keyrings in
# script/source/keys. Needs: bash, curl, jq, git, gzip, shasum, openssl,
# bsdtar, unzip, file, gpgv.
# =============================================================================

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REGISTRY="${VORPAL_SOURCE_REGISTRY:-${ROOT}/script/source/registry.json}"
LEDGER="${ROOT}/script/source/ledger.json"
KEYRING_DIR="${ROOT}/script/source/keys"
MIRROR_BASE="https://sdk.vorpal.build/source"
SYSTEMS=(aarch64-darwin aarch64-linux x86_64-darwin x86_64-linux)

die() {
    echo "error: $*" >&2
    exit 1
}

log() {
    echo "$*" >&2
}

sha256_file() {
    if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d" " -f1; return; fi
    shasum -a 256 "$1" | cut -d' ' -f1
}

sha256_stdin() {
    if command -v sha256sum >/dev/null; then sha256sum | cut -d" " -f1; return; fi
    shasum -a 256 | cut -d" " -f1
}

# -----------------------------------------------------------------------------
# lock: the original downloader, kept as the default subcommand
# -----------------------------------------------------------------------------

cmd_lock() {
    local lockfile="${PWD}/Vorpal.lock"
    local source_dir="${PWD}/source"

    [ -f "$lockfile" ] || die "Vorpal.lock not found at ${lockfile}"

    local pairs
    pairs=$(awk '
        /^\[\[sources\]\]/ { url=""; plat="" }
        /^path = "/ { url=$0; sub(/^path = "/, "", url); sub(/"$/, "", url) }
        /^platform = "/ { plat=$0; sub(/^platform = "/, "", plat); sub(/"$/, "", plat) }
        length(url) > 0 && length(plat) > 0 { print plat " " url; url=""; plat="" }
    ' "$lockfile" | sort -u)

    if [ -z "$pairs" ]; then
        echo "No sources found in Vorpal.lock"
        return 0
    fi

    local total count=0 failed_urls=()
    total=$(echo "$pairs" | wc -l | tr -d ' ')
    echo "Found ${total} unique source(s) to download"

    while IFS=' ' read -r platform url; do
        count=$((count + 1))
        local filename target_dir target_file
        filename=$(basename "${url%%\?*}")
        target_dir="${source_dir}/${platform}"
        target_file="${target_dir}/${filename}"

        if [ -f "$target_file" ]; then
            echo "[${count}/${total}] Skipping (exists): ${platform}/${filename}"
            continue
        fi

        mkdir -p "$target_dir"
        echo "[${count}/${total}] Downloading: ${platform}/${filename}"
        if ! curl -fSL -o "$target_file" "$url"; then
            echo "Warning: Failed to download ${platform}/${filename} from ${url}" >&2
            rm -f "$target_file"
            failed_urls+=("${platform} ${url}")
        fi
    done <<< "$pairs"

    if [ "${#failed_urls[@]}" -gt 0 ]; then
        echo "" >&2
        echo "Failed to download ${#failed_urls[@]} source(s):" >&2
        local entry
        for entry in "${failed_urls[@]}"; do
            echo "  - [${entry%% *}] ${entry#* }" >&2
        done
        exit 1
    fi

    echo "Done"
}

# -----------------------------------------------------------------------------
# digest: same algorithm as the agent (cli/src/command/start/agent.rs and
# cli/src/command/store/hashes.rs): unpack by detected type, take every path
# that resolves to a regular file, sort by path components, SHA-256 each file,
# concatenate the hex digests, and SHA-256 the concatenation.
# -----------------------------------------------------------------------------

inline_digest() {
    local file="$1" work names tar_bin
    local t
    for t in file unzip; do command -v "$t" >/dev/null || die "inline_digest needs $t"; done
    tar_bin=$(command -v bsdtar || command -v tar)
    work=$(mktemp -d)

    case "$(file -b --mime-type "$file")" in
        application/gzip | application/x-gzip | application/x-bzip2 | application/x-xz)
            names=$("$tar_bin" -tf "$file") || { rm -rf "$work"; return 1; }
            ;;
        application/zip)
            names=$(unzip -Z1 "$file") || { rm -rf "$work"; return 1; }
            ;;
        *)
            names=""
            ;;
    esac

    # Paths that differ only in case collapse on a case-insensitive file
    # system (the Linux kernel has several), so the digest would be wrong.
    if [ -n "$names" ] && [ "$(uname -s)" = "Darwin" ] &&
        [ -n "$(printf '%s\n' "$names" | sed 's|/$||' | tr '[:upper:]' '[:lower:]' | sort | uniq -d | head -1)" ]; then
        rm -rf "$work"
        log "$(basename "$file"): case-colliding paths; compute its digest on Linux (source.sh digest-fill in Lima)"
        return 5
    fi

    case "$(file -b --mime-type "$file")" in
        application/gzip | application/x-gzip | application/x-bzip2 | application/x-xz)
            "$tar_bin" -xf "$file" -C "$work" || { rm -rf "$work"; return 1; }
            ;;
        application/zip)
            unzip -q "$file" -d "$work" || { rm -rf "$work"; return 1; }
            ;;
        *)
            cp "$file" "${work}/$(basename "$file")" || { rm -rf "$work"; return 1; }
            ;;
    esac

    # Mapping "/" to \x01 before a byte sort orders paths component by
    # component, which is how Rust's PathBuf ordering compares them.
    local digest
    digest=$(
        set -o pipefail
        cd "$work" || exit 1
        find . -path ./.git -prune -o \( -type f -o -type l \) -print | sed 's|^\./||' |
            while IFS= read -r p; do if [ -f "$p" ]; then printf '%s\n' "$p"; fi; done |
            sed $'s|/|\x01|g' | LC_ALL=C sort | sed $'s|\x01|/|g' |
            while IFS= read -r p; do sha256_file "$p" || exit 1; done |
            tr -d '\n' | sha256_stdin
    ) || { rm -rf "$work"; return 1; }

    rm -rf "$work"
    echo "$digest"
}

cmd_digest() {
    [ $# -eq 1 ] && [ -f "$1" ] || die "usage: source.sh digest FILE"
    inline_digest "$1"
}

# Fills the inline digests the macOS stage deferred (case-colliding paths).
#   digest-fill --print [--dir DIR]   on Linux: print "<key> <digest>" lines;
#                                     reads only, so a read-only mount works.
#   digest-fill --apply FILE [--dir DIR]
#                                     on the host: write those lines into
#                                     DIR/manifest.json.
cmd_digest_fill() {
    local dir="${STAGE_DIR}" mode="" input=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --dir) dir="$2"; shift 2 ;;
            --print) mode="print"; shift ;;
            --apply) mode="apply"; input="$2"; shift 2 ;;
            *) die "unknown option: $1" ;;
        esac
    done
    local manifest="${dir}/manifest.json" key digest
    [ -f "$manifest" ] || die "no manifest at ${manifest}"

    case "$mode" in
        print)
            for key in $(jq -r '.records[] | select(.inline_digest == "") | .mirror_key' "$manifest"); do
                digest=$(inline_digest "${dir}/objects/${key}") || die "${key}: inline digest failed"
                printf '%s %s\n' "$key" "$digest"
            done
            ;;
        apply)
            [ -f "$input" ] || die "no digest file at ${input}"
            while read -r key digest; do
                printf '%s' "$digest" | grep -qE '^[0-9a-f]{64}$' || die "${key}: malformed digest"
                jq -e --arg k "$key" 'any(.records[]; .mirror_key == $k and .inline_digest == "")' \
                    "$manifest" >/dev/null || die "${key}: no deferred record"
                jq --arg k "$key" --arg d "$digest" \
                    '(.records[] | select(.mirror_key == $k) | .inline_digest) = $d' \
                    "$manifest" >"${manifest}.part"
                mv "${manifest}.part" "$manifest"
                log "${key}: ${digest}"
            done <"$input"
            ;;
        *) die "usage: source.sh digest-fill --print | --apply FILE [--dir DIR]" ;;
    esac
}

# -----------------------------------------------------------------------------
# Registry commands
# -----------------------------------------------------------------------------

STAGE_DIR="${ROOT}/.source-stage"

# jq helpers shared by every registry command.
JQ_LIB='
def vkey: [splits("[.-]") | tonumber? // 0];
def expand($v): reduce ($v | to_entries[]) as $e (.; gsub("\\{" + $e.key + "\\}"; ($e.value | tostring)));
def vars($s; $version; $target; $extra):
  {version: $version, target: $target, release: ($s.release // ""),
   major: ($version | split(".")[0]), minor: ($version | split(".")[0:2] | join(".")),
   nodot: ($version | gsub("\\."; ""))} + $extra;
def list($s): $s | split("\n") | map(select(length > 0));
'

F_KINDS=""
F_NOTKINDS=""
F_GROUPS=""
F_NAMES=""
F_NOTNAMES=""
F_TARGET=""
F_ADVISORY=""
F_RELEASE=""
F_DIR="$STAGE_DIR"
F_BUCKET=""
F_OUT="${ROOT}/provenance.json"
F_INCLUDE_CURRENT=0
F_PRESENT=0
F_TO=""

parse_filters() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --kind) F_KINDS="${F_KINDS}$2"$'\n'; shift 2 ;;
            --not-kind) F_NOTKINDS="${F_NOTKINDS}$2"$'\n'; shift 2 ;;
            --group) F_GROUPS="${F_GROUPS}$2"$'\n'; shift 2 ;;
            --name) F_NAMES="${F_NAMES}$2"$'\n'; shift 2 ;;
            --not-name) F_NOTNAMES="${F_NOTNAMES}$2"$'\n'; shift 2 ;;
            --target-version) F_TARGET="$2"; shift 2 ;;
            --advisory) F_ADVISORY="$2"; shift 2 ;;
            --release) F_RELEASE="$2"; shift 2 ;;
            --to) F_TO="$2"; shift 2 ;;
            --dir) F_DIR="$2"; shift 2 ;;
            --bucket) F_BUCKET="$2"; shift 2 ;;
            --out) F_OUT="$2"; shift 2 ;;
            --include-current) F_INCLUDE_CURRENT=1; shift ;;
            --present) F_PRESENT=1; shift ;;
            *) die "unknown option: $1" ;;
        esac
    done

    case "$F_DIR" in /*) ;; *) F_DIR="${PWD}/${F_DIR}" ;; esac

    if [ -n "$F_TARGET" ] && [ -z "$F_NAMES" ]; then
        die "--target-version needs --name (every source that shares its pins)"
    fi
    if [ -n "$F_ADVISORY" ] && [ -z "$F_TARGET" ]; then
        die "--advisory goes with --target-version"
    fi
    if [ -n "$F_ADVISORY" ] &&
        ! printf '%s' "$F_ADVISORY" | grep -qE '^https://www\.linuxfromscratch\.org/b?lfs/advisories/'; then
        die "--advisory must be an LFS or BLFS security advisory URL (https://www.linuxfromscratch.org/{lfs,blfs}/advisories/...)"
    fi
}

# A version override must name exactly one group of pin-sharing sources (the
# Rust components, glibc and its patch), must not touch Rust, which follows
# rust-toolchain.toml, and must name every member so stage covers every pin.
require_override_group() {
    local selected="$1" names groups missing
    printf '%s\n' "$selected" | jq -e 'select(.kind == "rust")' >/dev/null &&
        die "--target-version: Rust sources follow rust-toolchain.toml; edit that file instead"
    names=$(printf '%s\n' "$selected" | jq -sc 'map(.name)')
    groups=$(printf '%s\n' "$selected" | jq -sc 'map(.pins) | unique | length')
    [ "$groups" = "1" ] || die "--target-version: the named sources do not share pins; override one group at a time"
    missing=$(jq -r --argjson sel "$(printf '%s\n' "$selected" | jq -sc '.[0].pins')" --argjson names "$names" '
        .sources[] | select(.pins == $sel and (.name as $n | $names | index([$n]) | not)) | .name' "$REGISTRY")
    [ -z "$missing" ] || die "--target-version: also name the sources that share these pins: $(printf '%s ' $missing)"
}

# Prints one compact JSON object per selected registry source.
select_sources() {
    jq -c --arg kinds "$F_KINDS" --arg notkinds "$F_NOTKINDS" \
        --arg groups "$F_GROUPS" --arg names "$F_NAMES" --arg notnames "$F_NOTNAMES" "${JQ_LIB}"'
        .sources[]
        | select((list($kinds) | length == 0) or (.kind as $k | list($kinds) | index([$k])))
        | select(.kind as $k | list($notkinds) | index([$k]) | not)
        | select((list($groups) | length == 0) or any(.groups[]; . as $g | list($groups) | index([$g])))
        | select((list($names) | length == 0) or (.name as $n | list($names) | index([$n])))
        | select(.name as $n | list($notnames) | index([$n]) | not)
    ' "$REGISTRY"
}

# The LFS set moves as one: a selection that holds any LFS-book source must
# hold all of them.
require_whole_lfs_set() {
    local selected="$1" have want
    have=$(printf '%s\n' "$selected" | jq -r 'select(.book == "lfs" or .book == "blfs") | .name' | sort)
    [ -z "$have" ] && return 0
    want=$(jq -r '.sources[] | select(.book == "lfs" or .book == "blfs") | .name' "$REGISTRY" | sort)
    if [ "$have" != "$want" ]; then
        die "the book set (LFS and BLFS) moves as one; select every book source (e.g. --kind linux-vorpal) or none. Missing: $(comm -13 <(echo "$have") <(echo "$want") | tr '\n' ' ')"
    fi
}

# Per-run download cache, created before dispatch so subshells share it.
CACHE=""
cache_dir() {
    echo "$CACHE"
}

# Fetches URL once per run and prints the cached path.
fetch_cached() {
    local url="$1" path
    path="$(cache_dir)/$(printf '%s' "$url" | shasum -a 256 | cut -c1-32)"
    if [ ! -f "$path" ]; then
        curl -fsSL --retry 3 --retry-all-errors -o "${path}.part" "$url" || { rm -f "${path}.part"; return 1; }
        mv "${path}.part" "$path"
    fi
    echo "$path"
}

gh_api() {
    local auth=()
    [ -n "${GITHUB_TOKEN:-}" ] && auth=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
    local path
    path="$(cache_dir)/gh-$(printf '%s' "$1" | shasum -a 256 | cut -c1-32)"
    if [ ! -f "$path" ]; then
        curl -fsSL --retry 2 -H 'Accept: application/vnd.github+json' ${auth[@]+"${auth[@]}"} \
            -o "$path" "https://api.github.com/$1" || { rm -f "$path"; return 1; }
    fi
    cat "$path"
}

# Highest version among stdin lines.
max_version() {
    jq -R . | jq -rs "${JQ_LIB}"'map(select(length > 0)) | sort_by(vkey) | last // empty'
}

lfs_book_stable() {
    curl -fsSL "https://www.linuxfromscratch.org/lfs/view/stable-systemd/" |
        grep -oE 'Version [0-9]+\.[0-9]+(-rc[0-9]+)?-systemd' | head -1 | sed 's/^Version //'
}

lfs_wget_list() {
    fetch_cached "https://www.linuxfromscratch.org/lfs/view/stable-systemd/wget-list-systemd"
}

# Version of PACKAGE in the stable LFS book (case-sensitive basename prefix).
lfs_version() {
    local pkg="$1"
    sed 's|.*/||' "$(lfs_wget_list)" | grep -E "^${pkg}-[0-9]" | grep -vE '\.patch$' |
        grep -E '\.(tar\.(gz|xz|bz2)|tgz)$' | head -1 |
        sed -E "s/^${pkg}-//; s/\.(tar\.(gz|xz|bz2)|tgz)$//"
}

lfs_patch_url() {
    local pkg="$1" kind="$2"
    grep -E "/${pkg}-([0-9.]+-)?${kind}-[0-9]+\.patch$" "$(lfs_wget_list)" | head -1
}

blfs_page() {
    fetch_cached "https://www.linuxfromscratch.org/blfs/view/stable-systemd/$1"
}

blfs_version() {
    local name="$1" page="$2"
    grep -oE "https?://[^\"]+/${name}-[0-9][0-9.]*\.tar\.(gz|xz|bz2)" "$(blfs_page "$page")" | head -1 |
        sed -E "s|.*/${name}-||; s/\.tar\.(gz|xz|bz2)$//"
}

cacert_latest() {
    local date pem stamp epoch
    date=$(grep -oE 'cacert-[0-9]{4}-[0-9]{2}-[0-9]{2}\.pem' "$(fetch_cached https://curl.se/docs/caextract.html)" |
        sed 's/^cacert-//; s/\.pem$//' | sort | tail -1)
    pem=$(fetch_cached "https://curl.se/ca/cacert-${date}.pem")
    stamp=$(sed -n 's/^## Certificate data from Mozilla as of: //p' "$pem" | head -1)
    epoch=$(date -j -u -f '%a %b %e %T %Y GMT' "$stamp" +%s 2>/dev/null || date -u -d "$stamp" +%s)
    echo "${epoch} ${date}"
}

# Prints "TARGET [RELEASE] [DATE]" for a registry source, or fails.
latest_version() {
    local src="$1" method current
    method=$(jq -r '.latest.method' <<<"$src")
    current=$(jq -r '.version' <<<"$src")

    case "$method" in
        rust-toolchain)
            sed -n 's/^channel *= *"\(.*\)"/\1/p' "${ROOT}/rust-toolchain.toml"
            ;;
        github-release)
            local repo re
            repo=$(jq -r '.latest.repo' <<<"$src")
            re=$(jq -r '.latest.tag' <<<"$src")
            gh_api "repos/${repo}/releases?per_page=100" | jq -r --arg re "$re" "${JQ_LIB}"'
                [.[] | select((.draft | not) and (.prerelease | not)) | .tag_name
                 | select(test($re)) | match($re).captures[0].string] | sort_by(vkey) | last // empty'
            ;;
        go-proxy)
            local module re
            module=$(jq -r '.latest.module' <<<"$src")
            re=$(jq -r '.latest.tag' <<<"$src")
            curl -fsSL "https://proxy.golang.org/${module}/@v/list" | grep -E "$re" | sed 's/^v//' | max_version
            ;;
        go-dl)
            curl -fsSL 'https://go.dev/dl/?mode=json' |
                jq -r '[.[] | select(.stable) | .version] | first | ltrimstr("go")'
            ;;
        nodejs)
            # Newest LTS line: index.json lists releases newest first.
            curl -fsSL https://nodejs.org/dist/index.json |
                jq -r '[.[] | select(.lts != false)] | first | .version | ltrimstr("v")'
            ;;
        html-index)
            local url pattern
            url=$(jq -r '.latest.url' <<<"$src")
            pattern=$(jq -r '.latest.pattern' <<<"$src")
            curl -fsSL "$url" | grep -oE "$pattern" | sed -E "s/^${pattern}\$/\1/" | max_version
            ;;
        python-build-standalone)
            # cpython tracks the LFS book's Python, exactly, from the newest
            # python-build-standalone release that ships it for every system.
            local want repo tag sums missing
            want=$(lfs_version Python)
            [ -n "$want" ] || return 1
            repo=$(jq -r '.latest.repo' <<<"$src")
            for tag in $(gh_api "repos/${repo}/releases?per_page=30" |
                jq -r '.[] | select((.draft | not) and (.prerelease | not)) | .tag_name'); do
                sums=$(fetch_cached "https://github.com/${repo}/releases/download/${tag}/SHA256SUMS") || continue
                missing=$(jq -r '.systems | to_entries[] | .value' <<<"$src" | while read -r triple; do
                    grep -q " cpython-${want}+${tag}-${triple}-install_only.tar.gz$" "$sums" || echo "$triple"
                done)
                if [ -z "$missing" ]; then
                    echo "${want} ${tag}"
                    return 0
                fi
            done
            echo "blocked:no python-build-standalone release ships ${want} for every system" >&2
            return 1
            ;;
        lfs)
            lfs_version "$(jq -r '.latest.package' <<<"$src")"
            ;;
        lfs-patch)
            lfs_version "$(jq -r '.latest.package' <<<"$src")"
            ;;
        blfs)
            blfs_version "$(jq -r '.name' <<<"$src")" "$(jq -r '.latest.page' <<<"$src")"
            ;;
        curl-ca)
            cacert_latest
            ;;
        pinned)
            echo "$current"
            ;;
        *)
            die "$(jq -r .name <<<"$src"): unknown latest method ${method}"
            ;;
    esac
}

# Prints one JSON line per (system, url, key) a source would stage at VERSION.
source_entries() {
    local src="$1" version="$2" release="$3" extra="$4"
    jq -c --arg v "$version" --arg r "$release" --argjson extra "$extra" "${JQ_LIB}"'
        . as $s | (if $r != "" then .release = $r else . end) as $s
        | ($s.systems // {"any": ""}) | to_entries[]
        | . as $e
        | vars($s; $v; $e.value; $extra) as $vars
        | {system: $e.key, target: $e.value,
           url: ($s.upstream | expand($vars)),
           key: ($s.mirror | expand($vars))}
    ' <<<"$src"
}

# Fails when the URL is not fetchable (a one-byte ranged GET).
url_exists() {
    curl -fsL --retry 1 -r 0-0 -o /dev/null "$1"
}

# -----------------------------------------------------------------------------
# check: dry-run report and plan
# -----------------------------------------------------------------------------

cmd_check() {
    parse_filters "$@"
    local selected
    selected=$(select_sources)
    [ -n "$selected" ] || die "no registry source matches the filters"
    if [ -z "$F_ADVISORY" ]; then
        require_whole_lfs_set "$selected"
    fi
    [ -z "$F_TARGET" ] || require_override_group "$selected"

    # A new plan invalidates whatever the previous run staged and checked.
    rm -f "${F_DIR}/manifest.json" "${F_DIR}/mirror-status.json" "${F_DIR}/present.ok"

    local tracked stable
    tracked=$(jq -r '.lfs.tracked' "$LEDGER")
    if printf '%s\n' "$selected" | jq -e 'select(.kind == "linux-vorpal")' >/dev/null; then
        stable=$(lfs_book_stable)
        log "LFS book: tracked ${tracked}, stable ${stable}"
        [ "$tracked" = "$stable" ] || log "book changed: run script/source.sh book-diff and reconcile every changed section (see SKILL.md)"
    else
        stable="$tracked"
    fi
    local book_dir="${stable%-systemd}"

    mkdir -p "$F_DIR"
    local plan="${F_DIR}/plan.jsonl"
    : >"$plan"

    printf '%-20s %-13s %-14s %-14s %-9s %s\n' ARTIFACT KIND CURRENT TARGET STATUS AUTHORITY
    local src
    while IFS= read -r src; do
        local name kind current release out target new_release date status extra authority entries
        name=$(jq -r '.name' <<<"$src")
        kind=$(jq -r '.kind' <<<"$src")
        current=$(jq -r '.version' <<<"$src")
        release=$(jq -r '.release // ""' <<<"$src")
        status=""
        new_release="$release"
        date=""

        if [ -n "$F_TARGET" ]; then
            if jq -e '.track == "book" or (.track | startswith("lfs:"))' >/dev/null <<<"$src" && [ -z "$F_ADVISORY" ]; then
                die "${name}: a book-tracked source goes above the book only with --advisory URL (an LFS/BLFS security advisory)"
            fi
            target="$F_TARGET"
            [ -n "$F_RELEASE" ] && new_release="$F_RELEASE"
        elif out=$(latest_version "$src" 2>"$(cache_dir)/err") && [ -n "$out" ]; then
            target=$(awk '{print $1}' <<<"$out")
            case "$name" in
                cpython) new_release=$(awk '{print $2}' <<<"$out") ;;
                curl-cacert) date=$(awk '{print $2}' <<<"$out") ;;
            esac
        else
            target="$current"
            status="blocked"
            log "${name}: $(cat "$(cache_dir)/err") no target version found"
        fi

        if [ "$name" = "curl-cacert" ] && [ -z "$date" ]; then
            date=$(date -u -r "$target" +%Y-%m-%d 2>/dev/null || date -u -d "@${target}" +%Y-%m-%d)
        fi
        extra=$(jq -nc --arg book "$book_dir" --arg date "$date" '{book: $book, date: $date}')

        entries=$(source_entries "$src" "$target" "$new_release" "$extra" | jq -s -c .)

        if [ "$name" = "glibc-patch" ]; then
            local patch_url
            patch_url=$(lfs_patch_url glibc fhs)
            [ -n "$patch_url" ] || { status="blocked"; log "glibc-patch: the stable book lists no glibc fhs patch"; }
            entries=$(jq -c --arg u "$patch_url" 'map(.url = $u)' <<<"$entries")
        fi

        if [ -z "$status" ]; then
            if [ "$target" = "$current" ]; then
                # A release change alone would reuse the mirror key with new bytes.
                status="current"
                new_release="$release"
            elif [ "$(printf '%s\n%s\n' "$current" "$target" | max_version)" != "$target" ]; then
                status="held"
                log "${name}: ${target} is older than ${current}; never downgrade"
            else
                status="bump"
                local missing
                if jq -e 'has("archive")' >/dev/null <<<"$src"; then
                    local repo tag
                    repo=$(jq -r '.archive.repo' <<<"$src")
                    tag=$(jq -r --arg v "$target" '.archive.tag | gsub("\\{version\\}"; $v)' <<<"$src")
                    git ls-remote --exit-code --tags "$repo" "refs/tags/${tag}" >/dev/null 2>&1 ||
                        { status="blocked"; log "${name}: tag ${tag} not found at ${repo}"; }
                else
                    missing=$(jq -r '.[] | "\(.system) \(.url)"' <<<"$entries" | while read -r sys url; do
                        url_exists "$url" || echo "${sys}=${url}"
                    done)
                    if [ -n "$missing" ]; then
                        status="blocked"
                        log "${name} ${target}: upstream lacks $(echo "$missing" | tr '\n' ' ')(bump only when every declared system ships)"
                    fi
                fi
            fi
        fi

        authority=$(jq -r --arg v "$target" --arg r "$new_release" --argjson extra "$extra" "${JQ_LIB}"'
            .authority | expand(vars({release: $r}; $v; ""; $extra))' <<<"$src")

        printf '%-20s %-13s %-14s %-14s %-9s %s\n' "$name" "$kind" "$current" "$target" "$status" "$authority"

        jq -nc --argjson src "$src" --arg target "$target" --arg release "$new_release" \
            --arg status "$status" --arg authority "$authority" --argjson entries "$entries" \
            --arg advisory "$F_ADVISORY" --arg date "$date" --arg book "$book_dir" '
            {name: $src.name, kind: $src.kind, current: $src.version, target: $target,
             current_release: ($src.release // null), release: (if $release == "" then null else $release end),
             status: $status, authority: $authority, entries: $entries,
             vars: {book: $book, date: $date},
             advisory: (if $advisory == "" then null else $advisory end)}' >>"$plan"
    done <<<"$selected"

    jq -s 'sort_by(.name)' "$plan" >"${F_DIR}/plan.json"
    rm -f "$plan"
    log ""
    log "plan: ${F_DIR}/plan.json"
}

# -----------------------------------------------------------------------------
# stage: download, verify, and write objects/<key> plus manifest.json
# -----------------------------------------------------------------------------

# book_md5_for PAGE FILE: the MD5 on the first "MD5 sum" line after FILE's
# link on a BLFS package page, unless another link comes first.
book_md5_for() {
    awk -v b="$2" '
        !f && index($0, b) { f = 1; next }
        f && /href=/ { exit }
        f && /MD5 sum:/ { print; exit }
    ' "$1" | grep -oE '[0-9a-f]{32}' | head -1
}

# Prints the path of a keyring pinned in the registry; exits 4 when unpinned.
keyring_path() {
    local name="$1" want path got
    want=$(jq -r --arg k "$name" '.keys[$k].sha256 // empty' "$REGISTRY")
    path="${KEYRING_DIR}/${name}.gpg"
    if [ -z "$want" ] || [ ! -f "$path" ]; then
        log "keyring '${name}' is unpinned."
        log "  1. Source: $(jq -r --arg k "$name" '.keys[$k].url' "$REGISTRY"). Inspect its fingerprints."
        log "  2. Save it dearmored as ${path} (gpg --dearmor)."
        log "  3. Ask the operator to pin: jq '.keys.${name}.sha256 = \"<sha256 of ${path}>\"' in registry.json."
        exit 4
    fi
    got=$(sha256_file "$path")
    [ "$got" = "$want" ] || die "keyring ${path} sha256 ${got} != pinned ${want}"
    echo "$path"
}

# gpg_verify KEYRING SIG FILE          detached signature
# gpg_verify KEYRING CLEARSIGNED --output PLAIN
#                                      writes only the signed text to PLAIN
gpg_verify() {
    local keyring="$1" sig="$2" file="${3:-}" home
    home=$(mktemp -d)
    if [ "$file" = "--output" ]; then
        rm -f "$4"
        GNUPGHOME="$home" gpgv --keyring "$keyring" --output "$4" "$sig" >/dev/null 2>&1
    else
        GNUPGHOME="$home" gpgv --keyring "$keyring" "$sig" "$file" >/dev/null 2>&1
    fi
    local rc=$?
    rm -rf "$home"
    return $rc
}

# Looks NAME up in a "<hex>  <name>" list.
hash_from_list() {
    awk -v f="$2" '$2 == f || $2 == "*" f || $2 == "./" f { print $1; exit }' "$1"
}

# Verifies FILE with the source's declared authority (verify[0]) and prints
# '{"method":..,"url":..,"value":..}'. Never falls back to a weaker method.
verify_file() {
    local src="$1" file="$2" url="$3" plan_vars="$4" target="$5" release="$6"
    local spec method base sha expected list_url sig kr
    spec=$(jq -c --arg v "$target" --arg r "$release" --arg up "$url" --argjson extra "$plan_vars" \
        --arg base "$(basename "$url")" "${JQ_LIB}"'
        .verify[0] | with_entries(.value |= (if type == "string"
            then expand(vars({release: $r}; $v; ""; $extra) + {upstream: $up, file: $base}) else . end))' <<<"$src")
    method=$(jq -r '.method' <<<"$spec")
    base=$(basename "$url")
    sha=$(sha256_file "$file")

    case "$method" in
        gpg)
            sig="$(cache_dir)/$(basename "$(jq -r '.signature' <<<"$spec")")"
            curl -fsSL -o "$sig" "$(jq -r '.signature' <<<"$spec")" || die "${base}: signature missing"
            kr=$(keyring_path "$(jq -r '.keyring' <<<"$spec")") || exit $?
            gpg_verify "$kr" "$sig" "$file" || die "${base}: bad signature"
            jq -nc --arg u "$(jq -r '.signature' <<<"$spec")" --arg v "$sha" '{method: "gpg", url: $u, value: $v}'
            ;;
        sha256-list)
            list_url=$(jq -r '.url' <<<"$spec")
            local list
            list=$(fetch_cached "$list_url") || die "${base}: checksum list missing at ${list_url}"
            if jq -e '.signature' >/dev/null <<<"$spec"; then
                sig="$(cache_dir)/list.sig"
                curl -fsSL -o "$sig" "$(jq -r '.signature' <<<"$spec")" || die "${base}: list signature missing"
                kr=$(keyring_path "$(jq -r '.keyring' <<<"$spec")") || exit $?
                gpg_verify "$kr" "$sig" "$list" || die "${base}: bad list signature"
            elif jq -e '.clearsigned' >/dev/null <<<"$spec"; then
                kr=$(keyring_path "$(jq -r '.keyring' <<<"$spec")") || exit $?
                # Read hashes only from the signed text, never from the raw
                # file, where unsigned lines can surround the signed block.
                local plain
                plain="$(cache_dir)/plain-$(basename "$list")"
                gpg_verify "$kr" "$list" --output "$plain" || die "${base}: bad clearsigned list"
                list="$plain"
            fi
            expected=$(hash_from_list "$list" "$base")
            [ -n "$expected" ] || die "${base}: not listed in ${list_url}"
            [ "$expected" = "$sha" ] || die "${base}: sha256 ${sha} != ${expected} from ${list_url}"
            jq -nc --arg u "$list_url" --arg v "$expected" '{method: "sha256-list", url: $u, value: $v}'
            ;;
        sha256-sidecar)
            list_url=$(jq -r '.url' <<<"$spec")
            expected=$(curl -fsSL "$list_url" | awk '{print $1; exit}') || die "${base}: sidecar missing"
            [ "$expected" = "$sha" ] || die "${base}: sha256 ${sha} != ${expected} from ${list_url}"
            jq -nc --arg u "$list_url" --arg v "$expected" '{method: "sha256-sidecar", url: $u, value: $v}'
            ;;
        sha256-json)
            list_url=$(jq -r '.url' <<<"$spec")
            expected=$(jq -r "$(jq -r '.filter' <<<"$spec")" "$(fetch_cached "$list_url")") || die "${base}: checksum json missing"
            [ "$expected" = "$sha" ] || die "${base}: sha256 ${sha} != ${expected} from ${list_url}"
            jq -nc --arg u "$list_url" --arg v "$expected" '{method: "sha256-json", url: $u, value: $v}'
            ;;
        github-asset-digest)
            local repo tag
            repo=$(jq -r '.repo' <<<"$spec")
            tag=$(jq -r '.tag' <<<"$spec")
            expected=$(gh_api "repos/${repo}/releases/tags/${tag}" |
                jq -r --arg n "$base" '.assets[] | select(.name == $n) | .digest // empty' | sed 's/^sha256://')
            [ -n "$expected" ] || die "${base}: GitHub publishes no asset digest"
            [ "$expected" = "$sha" ] || die "${base}: sha256 ${sha} != GitHub asset digest ${expected}"
            jq -nc --arg u "https://api.github.com/repos/${repo}/releases/tags/${tag}" --arg v "$expected" \
                '{method: "github-asset-digest", url: $u, value: $v}'
            ;;
        book-md5)
            local book md5 page
            book=$(jq -r '.book // empty' <<<"$spec")
            if [ -n "$book" ]; then
                page=$(fetch_cached "$book")
                # Only the "MD5 sum" line that follows this file's own link,
                # before any other link, counts.
                expected=$(book_md5_for "$page" "$base")
            else
                book="https://www.linuxfromscratch.org/lfs/view/stable-systemd/md5sums"
                expected=$(hash_from_list "$(fetch_cached "$book")" "$base")
            fi
            [ -n "$expected" ] || die "${base}: no book MD5 at ${book}"
            md5=$(md5 -q "$file" 2>/dev/null || md5sum "$file" | cut -d' ' -f1)
            [ "$expected" = "$md5" ] || die "${base}: md5 ${md5} != book ${expected}"
            jq -nc --arg u "$book" --arg v "$expected" '{method: "book-md5", url: $u, value: $v}'
            ;;
        *)
            die "${base}: authority method ${method} is not a file check"
            ;;
    esac
}

# Builds a deterministic tag archive and checks the commit against the
# declared ref authority. Prints the authority JSON.
stage_archive() {
    local src="$1" target="$2" out="$3"
    local repo tag prefix work commit expected spec authority_url
    repo=$(jq -r '.archive.repo' <<<"$src")
    tag=$(jq -r --arg v "$target" '.archive.tag | gsub("\\{version\\}"; $v)' <<<"$src")
    prefix=$(jq -r --arg v "$target" '.archive.prefix | gsub("\\{version\\}"; $v)' <<<"$src")
    work=$(mktemp -d)

    git clone --quiet --depth 1 --branch "$tag" "$repo" "${work}/src" 2>/dev/null || die "${repo}: clone of ${tag} failed"
    commit=$(git -C "${work}/src" rev-parse HEAD)

    spec=$(jq -c --arg v "$target" '.verify[0] | with_entries(.value |= (if type == "string" then gsub("\\{version\\}"; $v) else . end))' <<<"$src")
    case "$(jq -r '.ref' <<<"$spec")" in
        github)
            local gh_repo obj
            gh_repo=$(jq -r '.repo' <<<"$spec")
            authority_url="https://api.github.com/repos/${gh_repo}/git/ref/tags/${tag}"
            obj=$(gh_api "repos/${gh_repo}/git/ref/tags/${tag}") || die "${tag}: no GitHub tag ref"
            expected=$(jq -r '.object.sha' <<<"$obj")
            if [ "$(jq -r '.object.type' <<<"$obj")" = "tag" ]; then
                expected=$(gh_api "repos/${gh_repo}/git/tags/${expected}" | jq -r '.object.sha')
            fi
            ;;
        go-proxy)
            authority_url="https://proxy.golang.org/$(jq -r '.module' <<<"$spec")/@v/$(jq -r '.module_version' <<<"$spec").info"
            expected=$(curl -fsSL "$authority_url" | jq -r '.Origin.Hash // empty')
            ;;
        *) die "${tag}: unknown commit authority" ;;
    esac
    [ -n "$expected" ] && [ "$expected" = "$commit" ] || die "${repo} ${tag}: commit ${commit} != authority ${expected:-none}"

    git -C "${work}/src" -c core.autocrlf=false archive --format=tar --prefix="$prefix" "$commit" | gzip -n -9 >"$out" ||
        die "${repo} ${tag}: git archive failed"
    rm -rf "$work"
    jq -nc --arg u "$authority_url" --arg v "$commit" --arg t "refs/tags/${tag}" \
        '{method: "git-commit", url: $u, value: $v, ref: $t}'
}

cmd_stage() {
    parse_filters "$@"
    local plan="${F_DIR}/plan.json"
    [ -f "$plan" ] || die "no plan at ${plan}; run check first"
    # A failed or partial stage must never leave an older manifest behind.
    rm -f "${F_DIR}/manifest.json" "${F_DIR}/mirror-status.json" "${F_DIR}/present.ok"
    # Blocked and held rows are reported and skipped. The LFS set moves as
    # one: a blocked LFS row holds back every LFS bump; other rows still stage.
    local lfs_names lfs_hold=0
    lfs_names=$(jq -c '[.sources[] | select(.book == "lfs" or .book == "blfs") | .name]' "$REGISTRY")
    if jq -e --argjson lfs "$lfs_names" 'any(.[]; .status == "blocked" and (.name as $n | $lfs | index([$n])))
        and any(.[]; .status == "bump" and (.name as $n | $lfs | index([$n])))' "$plan" >/dev/null; then
        lfs_hold=1
        log "an LFS source is blocked; the LFS set moves as one, so every LFS bump is held (see SKILL.md)"
    fi
    jq -r '.[] | select(.status == "blocked" or .status == "held") | "skip \(.name) (\(.status))"' "$plan" >&2
    # Tag archives at their current version were packed by another gzip, so
    # the rebuilt bytes differ from the mirror's; they are baselined when bumped.
    jq -r --argjson cur "$F_INCLUDE_CURRENT" '.[] | select($cur == 1 and .status == "current") | .name' "$plan" |
        while IFS= read -r n; do
            if jq -e --arg n "$n" '.sources[] | select(.name == $n) | has("archive")' "$REGISTRY" >/dev/null; then
                log "skip ${n} (current tag archive: not reproducible byte for byte)"
            fi
        done
    local selector
    selector='.[] | select(.status == "bump" or ($cur == 1 and .status == "current"))
        | select(($hold == 0) or ((.name as $n | $lfs | index([$n])) | not) or .status == "current")
        | select(.status == "bump" or ((.name as $n | $archives | index([$n])) | not))'
    local archives
    archives=$(jq -c '[.sources[] | select(has("archive")) | .name]' "$REGISTRY")
    [ "$lfs_hold" -eq 0 ] || jq -r --argjson lfs "$lfs_names" '.[] | select(.status == "bump" and (.name as $n | $lfs | index([$n]))) | "skip \(.name) (LFS set held)"' "$plan" >&2

    # Every keyring the selection needs, checked before any download.
    local unpinned=0 kr
    for kr in $(jq -r --argjson cur "$F_INCLUDE_CURRENT" --argjson hold "$lfs_hold" --argjson lfs "$lfs_names" --argjson archives "$archives" \
        "$selector"' | .name' "$plan" | while IFS= read -r n; do
            jq -r --arg n "$n" '.sources[] | select(.name == $n) | .verify[0].keyring // empty' "$REGISTRY"
        done | sort -u); do
        if [ -z "$(jq -r --arg k "$kr" '.keys[$k].sha256 // empty' "$REGISTRY")" ] || [ ! -f "${KEYRING_DIR}/${kr}.gpg" ]; then
            log "keyring '${kr}' is unpinned. Source: $(jq -r --arg k "$kr" '.keys[$k].url' "$REGISTRY"). Inspect its fingerprints, save it dearmored as ${KEYRING_DIR}/${kr}.gpg, and ask the operator to pin its sha256 in registry.json"
            unpinned=1
        fi
    done
    [ "$unpinned" -eq 0 ] || exit 4

    # A failed or partial stage must never leave an older manifest behind.
    rm -f "${F_DIR}/manifest.json" "${F_DIR}/mirror-status.json" "${F_DIR}/present.ok"
    mkdir -p "${F_DIR}/objects"
    local records="${F_DIR}/records.jsonl"
    : >"$records"

    local item
    jq -c --argjson cur "$F_INCLUDE_CURRENT" --argjson hold "$lfs_hold" --argjson lfs "$lfs_names" --argjson archives "$archives" \
        "$selector" "$plan" |
        while IFS= read -r item; do
            local name src target release plan_vars entry
            name=$(jq -r '.name' <<<"$item")
            target=$(jq -r '.target' <<<"$item")
            release=$(jq -r '.release // ""' <<<"$item")
            plan_vars=$(jq -c '.vars' <<<"$item")
            src=$(jq -c --arg n "$name" '.sources[] | select(.name == $n)' "$REGISTRY")

            jq -c '.entries[]' <<<"$item" | while IFS= read -r entry; do
                local key url system out authority sha inline triple
                key=$(jq -r '.key' <<<"$entry")
                url=$(jq -r '.url' <<<"$entry")
                system=$(jq -r '.system' <<<"$entry")
                triple=$(jq -r --arg s "$system" '{"aarch64-darwin":"aarch64-apple-darwin","aarch64-linux":"aarch64-unknown-linux-gnu","x86_64-darwin":"x86_64-apple-darwin","x86_64-linux":"x86_64-unknown-linux-gnu"}[$s] // "any"' <<<'null')
                out="${F_DIR}/objects/${key}"
                log "stage ${key}"

                if jq -e 'has("archive")' >/dev/null <<<"$src"; then
                    authority=$(stage_archive "$src" "$target" "${out}.part")
                else
                    curl -fsSL --retry 2 -o "${out}.part" "$url" || die "${key}: download failed from ${url}"
                    authority=$(verify_file "$src" "${out}.part" "$url" "$plan_vars" "$target" "$release")
                fi
                mv "${out}.part" "$out"

                sha=$(sha256_file "$out")
                local rc=0
                inline=$(inline_digest "$out") || rc=$?
                case "$rc" in
                    0) [ "$sha" != "$inline" ] || die "${key}: upstream sha256 equals inline digest" ;;
                    5) inline="" ;;
                    *) die "${key}: inline digest failed" ;;
                esac

                jq -nc --arg artifact "$name" --arg version "$target" --arg system "$system" --arg triple "$triple" \
                    --arg up "$url" --arg sha "$sha" --arg key "$key" --arg mirror "${MIRROR_BASE}/${key}" \
                    --arg inline "$inline" --argjson authority "$authority" '
                    {artifact: $artifact, version: $version, system: $system, triple: $triple,
                     upstream_url: $up, upstream_sha256: $sha, mirror_key: $key, mirror_url: $mirror,
                     inline_digest: $inline, authority: $authority}' >>"$records"
            done
        done

    jq -s '{schema: 1, mirror: "'"${MIRROR_BASE}"'", records: sort_by(.mirror_key)}' "$records" >"${F_DIR}/manifest.json"
    rm -f "$records"
    log "manifest: ${F_DIR}/manifest.json ($(jq '.records | length' "${F_DIR}/manifest.json") objects)"
}

# -----------------------------------------------------------------------------
# mirror-check, upload-cmd, provenance
# -----------------------------------------------------------------------------

cmd_mirror_check() {
    parse_filters "$@"
    local manifest="${F_DIR}/manifest.json" status="${F_DIR}/mirror-status.jsonl" conflict=0
    [ -f "$manifest" ] || die "no manifest at ${manifest}; run stage first"

    if [ "$F_PRESENT" -eq 1 ]; then
        local missing=0 key code
        for key in $(jq -r '.records[].mirror_key' "$manifest"); do
            code=$(curl -sIL -o /dev/null -w '%{http_code}' "${MIRROR_BASE}/${key}" || echo 000)
            case "$code" in
                200) printf '%-10s %s\n' present "$key" ;;
                403 | 404) printf '%-10s %s\n' absent "$key"; missing=1 ;;
                *) die "${key}: unexpected HTTP ${code}" ;;
            esac
        done
        [ "$missing" -eq 0 ] || { log "some staged keys are not on the mirror yet"; exit 6; }
        : >"${F_DIR}/present.ok"
        return 0
    fi

    rm -f "${F_DIR}/present.ok"
    : >"$status"
    local rec
    while IFS= read -r rec; do
        local key url sha code tmp state got=""
        key=$(jq -r '.mirror_key' <<<"$rec")
        url=$(jq -r '.mirror_url' <<<"$rec")
        sha=$(jq -r '.upstream_sha256' <<<"$rec")
        tmp=$(mktemp)
        code=$(curl -sL -o "$tmp" -w '%{http_code}' "$url" || echo 000)
        case "$code" in
            200)
                got=$(sha256_file "$tmp")
                if [ "$got" = "$sha" ]; then state="same"; else state="conflict"; conflict=1; fi
                ;;
            403 | 404) state="absent" ;;
            *) rm -f "$tmp"; die "${url}: unexpected HTTP ${code}" ;;
        esac
        rm -f "$tmp"
        printf '%-10s %s\n' "$state" "$key"
        jq -nc --arg k "$key" --arg s "$state" --arg g "$got" '{key: $k, state: $s, mirror_sha256: $g}' >>"$status"
    done < <(jq -c '.records[]' "$manifest")

    jq -s 'sort_by(.key)' "$status" >"${F_DIR}/mirror-status.json"
    rm -f "$status"
    if [ "$conflict" -eq 1 ]; then
        log "a mirror object differs from the staged bytes; stop and report it (never overwrite)"
        exit 3
    fi
}

cmd_upload_cmd() {
    parse_filters "$@"
    [ -n "$F_BUCKET" ] || die "usage: source.sh upload-cmd --bucket BUCKET [--dir DIR]"
    local status="${F_DIR}/mirror-status.json"
    [ -f "$status" ] || die "no mirror status; run mirror-check first"
    jq -e 'any(.[]; .state == "conflict")' "$status" >/dev/null && die "mirror conflict recorded; no upload"

    local key
    jq -r '.[] | select(.state == "absent") | .key' "$status" | while IFS= read -r key; do
        local file b64
        file="${F_DIR}/objects/${key}"
        b64=$(openssl dgst -sha256 -binary "$file" | base64)
        printf "aws s3api put-object --bucket %q --key %q --body %q --if-none-match '*' --checksum-sha256 %q\n" \
            "$F_BUCKET" "source/${key}" "$file" "$b64"
    done
}

cmd_provenance() {
    parse_filters "$@"
    local manifest="${F_DIR}/manifest.json"
    [ -f "$manifest" ] || die "no manifest at ${manifest}"
    local existing='{"records": []}'
    [ -f "$F_OUT" ] && existing=$(cat "$F_OUT")

    # A recorded object never changes: the same mirror URL with another
    # upstream hash stops the merge.
    local clash
    clash=$(jq -rn --argjson old "$existing" --slurpfile new "$manifest" '
        [$old.records[] as $o | $new[0].records[] | select(.mirror_url == $o.mirror_url and .upstream_sha256 != $o.upstream_sha256) | .mirror_url] | .[]')
    [ -z "$clash" ] || die "provenance conflict for: ${clash}"
    # Only objects the mirror holds with the staged bytes get a record:
    # mirror-check said "same", or "absent" and mirror-check --present (run
    # after the upload) then found them.
    local status="${F_DIR}/mirror-status.json" unconfirmed
    [ -f "$status" ] || die "no mirror status; run mirror-check (and mirror-check --present after an upload) first"
    [ -f "${F_DIR}/present.ok" ] || ! jq -e 'any(.[]; .state == "absent")' "$status" >/dev/null ||
        die "objects were absent at mirror-check; run mirror-check --present after the upload first"
    unconfirmed=$(jq -rn --slurpfile st "$status" --slurpfile m "$manifest" '
        ($st[0] | map({(.key): .state}) | add // {}) as $s
        | $m[0].records[] | select(($s[.mirror_key] // "missing") as $x | $x != "same" and $x != "absent") | .mirror_key')
    [ -z "$unconfirmed" ] || die "mirror state is not same or uploaded for: ${unconfirmed}"
    jq -e 'any(.records[]; .inline_digest == "")' "$manifest" >/dev/null &&
        die "manifest has deferred inline digests; run source.sh digest-fill --print on Linux, then digest-fill --apply on the host"

    jq -n --argjson old "$existing" --slurpfile new "$manifest" '
        {records: ([$old.records[], $new[0].records[]] | unique_by(.mirror_url) | sort_by(.mirror_url))}' >"${F_OUT}.part"
    mv "${F_OUT}.part" "$F_OUT"
    log "provenance: ${F_OUT} ($(jq '.records | length' "$F_OUT") records)"
}

# -----------------------------------------------------------------------------
# pins: verify every SDK pin; set-version: rewrite them
# -----------------------------------------------------------------------------

# Prints "file<TAB>line" for every pinned line of a source.
pin_lines() {
    local pin file marker after
    while IFS= read -r pin; do
        file=$(jq -r '.file' <<<"$pin")
        marker=$(jq -r '.marker' <<<"$pin")
        after=$(jq -r '.after // ""' <<<"$pin")
        [ -f "${ROOT}/${file}" ] || { printf '%s\t%s\n' "$file" "<missing file>"; continue; }
        awk -v m="$marker" -v a="$after" -v f="$file" '
            { t = $0; sub(/^[ \t]+/, "", t) }
            a != "" && prev && index(t, m) == 1 { print f "\t" NR "\t" t; n++ }
            a == "" && index(t, m) == 1 { print f "\t" NR "\t" t; n++ }
            { prev = (a != "" && index(t, a) == 1) }
            END { if (n == 0) print f "\t0\t<marker not found: " m ">" }
        ' "${ROOT}/${file}"
    done < <(jq -c '.pins[]' <<<"$1")
}

cmd_pins() {
    parse_filters "$@"
    local bad=0 src
    while IFS= read -r src; do
        local name version
        name=$(jq -r '.name' <<<"$src")
        version=$(jq -r '.version' <<<"$src")
        while IFS=$'\t' read -r file line text; do
            if [ "$line" = "0" ] || ! printf '%s' "$text" | grep -qE "(^|[^0-9A-Za-z.])$(printf '%s' "$version" | sed 's/\./\\./g')([^0-9A-Za-z.]|\.[^0-9]|\.?$)"; then
                echo "DRIFT ${name} ${version}: ${file}:${line}: ${text}"
                bad=1
            fi
        done < <(pin_lines "$src")
    done < <(select_sources)
    [ "$bad" -eq 0 ] && log "every pin matches registry.json"
    return "$bad"
}

cmd_set_version() {
    parse_filters "$@"
    [ -n "$F_TO" ] && [ "$(printf '%s' "$F_NAMES" | grep -c .)" -eq 1 ] || die "usage: source.sh set-version --name NAME --to VERSION [--release R]"
    local src name old kind
    src=$(select_sources)
    name=$(jq -r '.name' <<<"$src")
    old=$(jq -r '.version' <<<"$src")
    kind=$(jq -r '.kind' <<<"$src")

    local pin file marker after
    while IFS= read -r pin; do
        file=$(jq -r '.file' <<<"$pin")
        # Go, TypeScript, and Python linux-vorpal sources are generated: edit
        # the Rust file and run sdk/codegen.
        if [ "$kind" = "linux-vorpal" ] && [ "${file#sdk/rust/}" = "$file" ]; then
            continue
        fi
        marker=$(jq -r '.marker' <<<"$pin")
        after=$(jq -r '.after // ""' <<<"$pin")
        awk -v m="$marker" -v a="$after" -v o="$old" -v n="$F_TO" '
            function swap(s,   i, out) {
                out = ""
                while ((i = index(s, o)) > 0) { out = out substr(s, 1, i - 1) n; s = substr(s, i + length(o)) }
                return out s
            }
            { t = $0; sub(/^[ \t]+/, "", t) }
            (a == "" || prev) && index(t, m) == 1 { $0 = swap($0) }
            { prev = (a != "" && index(t, a) == 1); print }
        ' "${ROOT}/${file}" >"${ROOT}/${file}.part"
        mv "${ROOT}/${file}.part" "${ROOT}/${file}"
    done < <(jq -c '.pins[]' <<<"$src")

    # Sources that share pins (the seven Rust components, glibc and its patch,
    # unzip and its patches) move together.
    jq --arg n "$name" --arg v "$F_TO" --arg r "$F_RELEASE" --argjson pins "$(jq -c '.pins' <<<"$src")" '
        .sources |= map(if .name == $n or .pins == $pins
            then .version = $v | (if $r != "" and has("release") then .release = $r else . end) else . end)
    ' "$REGISTRY" >"${REGISTRY}.part"
    mv "${REGISTRY}.part" "$REGISTRY"
    log "${name}: ${old} -> ${F_TO} (registry and pins)"
}

# --- Book commands ------------------------------------------------------------
# The linux-vorpal stage scripts implement commands from the tracked LFS and
# BLFS books. script/source/book holds the extracted commands of every book
# section the map names, and ledger.json ties them together:
#   book_sections  mapped sections, LFS in book order then BLFS: id, sha (of
#                  the extracted commands), status (unaudited | unreconciled |
#                  audited), disposition (edited | unchanged | deviation),
#                  reason, changed (the commands or their place in the book
#                  changed at the last --accept)
#   book_skipped   sections the scripts do not implement: {id, reason, sha,
#                  status} (snapshotted, so a change shows) or {prefix, reason}
#   stage_blocks   every "##"/"###" block of the stage scripts, then the step
#                  sequence of linux_vorpal.rs, in order: file, heading, sha,
#                  and sections or local (reason)

LV_SCRIPT_DIR="${ROOT}/sdk/rust/src/artifact/linux_vorpal/script"
LV_FILES=(setup stage_01 stage_02 stage_03 stage_04 stage_05)
LV_STEPS="${ROOT}/sdk/rust/src/artifact/linux_vorpal.rs"
BOOK_SNAPSHOTS="${ROOT}/script/source/book"
EMPTY_SHA="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"

book_url() {
    echo "https://www.linuxfromscratch.org/$1/view/$2/"
}

# Section id (lfs:chapter05/glibc) -> its page URL in VERSION.
section_url() {
    echo "$(book_url "${1%%:*}" "$2")${1#*:}.html"
}

snapshot_rel() {
    echo "${1%%:*}/${1#*:}.sh"
}

# Ordered chapter 4-8 section ids of the LFS book at VERSION.
lfs_toc() {
    local page
    page=$(fetch_cached "$(book_url lfs "$1")index.html") || die "cannot fetch the LFS $1 table of contents"
    grep -oE 'href="chapter0[4-8]/[^"#]+\.html"' "$page" | sed -E 's/^href="(.*)\.html"$/lfs:\1/' | awk '!seen[$0]++'
}

# Fetches section ID at VERSION and prints the cached page path. Returns 2
# when the book has no such page (HTTP 404); dies on any other failure.
fetch_section() {
    local url
    url=$(section_url "$1" "$2")
    fetch_cached "$url" && return 0
    [ "$(curl -sS -o /dev/null -w '%{http_code}' --retry 2 "$url")" = 404 ] || die "cannot fetch ${url}; retry"
    return 2
}

# Prints the commands of an LFS/BLFS page: the text of every
# <pre class="userinput"> and <pre class="root"> block (root blocks marked),
# tags removed, entities decoded, blocks separated by one blank line. Tags may
# span lines, so the page is read whole.
book_commands() {
    awk '
        { s = s $0 "\n" }
        END {
            u = "<pre class=\"userinput\">"; r = "<pre class=\"root\">"
            while (1) {
                i = index(s, u); j = index(s, r)
                if (i == 0 && j == 0) break
                if (i == 0 || (j > 0 && j < i)) { s = substr(s, j + length(r)); root = 1 }
                else { s = substr(s, i + length(u)); root = 0 }
                if ((k = index(s, "</pre>")) == 0) break
                b = substr(s, 1, k - 1); s = substr(s, k + 6)
                gsub(/<[^>]*>/, "", b)
                gsub(/&lt;/, "<", b); gsub(/&gt;/, ">", b); gsub(/&quot;/, "\"", b)
                gsub(/&#39;/, "\047", b); gsub(/&nbsp;/, " ", b); gsub(/&amp;/, "\\&", b)
                sub(/\n+$/, "", b)
                if (n++) printf "\n"
                if (root) print "# (as root)"
                print b
            }
        }' "$1"
}

# Writes the snapshot of ID at VERSION from PAGE to DEST and prints the
# sha256 of its commands. The four-line header is not hashed.
write_snapshot() {
    local id="$1" version="$2" page="$3" dest="$4" title
    case "${id%%:*}" in
        lfs) title="Linux From Scratch" ;;
        blfs) title="Beyond Linux From Scratch" ;;
        *) die "bad section id: ${id}" ;;
    esac
    mkdir -p "$(dirname "$dest")"
    book_commands "$page" >"${dest}.body"
    {
        echo "# ${title} ${version}: ${id#*:}"
        echo "# $(section_url "$id" "$version")"
        echo "# Commands extracted under the book's MIT License; see script/source/book/LICENSE."
        echo
        cat "${dest}.body"
    } >"$dest"
    sha256_file "${dest}.body"
    rm -f "${dest}.body"
}

# sha256 of a snapshot's commands (everything after the header).
snapshot_sha() {
    tail -n +5 "$1" >"${CACHE}/snapshot.body"
    sha256_file "${CACHE}/snapshot.body"
}

# Prints one JSON object per stage block: file, heading, sha, path (the block
# text). Each stage script's formatdoc! body splits at its "##" and "###"
# headings; a block runs from its heading to the next heading or the end of
# the body, and lines before the first heading form "(preamble)". Then the
# step sequence of linux_vorpal.rs, from "let step_environments" to the line
# before 'let name = "linux-vorpal";', is the block linux_vorpal "(steps)".
# Lines lose trailing whitespace; trailing blank lines are dropped.
# sdk/python/tests/test_book_map.py repeats these rules; change both together.
stage_blocks() {
    local work f file heading path
    work=$(mktemp -d "${CACHE}/blocks.XXXXXX")
    : >"${work}/index"
    for f in "${LV_FILES[@]}"; do
        [ -f "${LV_SCRIPT_DIR}/${f}.rs" ] || die "missing stage script ${LV_SCRIPT_DIR}/${f}.rs"
        awk -v dir="$work" -v file="$f" '
            function flush(   i, path) {
                while (n > 0 && lines[n] == "") n--
                if (n > 0) {
                    path = sprintf("%s/%s.%03d", dir, file, ++k)
                    for (i = 1; i <= n; i++) print lines[i] > path
                    close(path)
                    printf "%s\t%s\t%s\n", file, head, path >> (dir "/index")
                    close(dir "/index")
                }
                n = 0
            }
            !p { if (index($0, "formatdoc! {\"") > 0) { p = 1; head = "(preamble)" }; next }
            { line = $0; sub(/[ \t]+$/, "", line) }
            line ~ /^[ \t]*###? / { flush(); head = line; sub(/^[ \t]*#+[ \t]*/, "", head) }
            { lines[++n] = line }
            line ~ /[^\\]",?$/ { flush(); done = 1; exit }
            END {
                if (!done) { print "no terminated formatdoc! body in " file ".rs" > "/dev/stderr"; exit 1 }
            }
        ' "${LV_SCRIPT_DIR}/${f}.rs" || die "cannot split ${f}.rs into blocks"
    done
    awk -v path="${work}/steps" '
        index($0, "let step_environments") > 0 { p = 1 }
        p && index($0, "let name = \"linux-vorpal\";") > 0 { done = 1; exit }
        p { line = $0; sub(/[ \t]+$/, "", line); lines[++n] = line }
        END {
            if (!done) { print "no step sequence in linux_vorpal.rs" > "/dev/stderr"; exit 1 }
            while (n > 0 && lines[n] == "") n--
            for (i = 1; i <= n; i++) print lines[i] > path
        }
    ' "$LV_STEPS" || die "cannot read the step sequence of ${LV_STEPS}"
    printf 'linux_vorpal\t(steps)\t%s\n' "${work}/steps" >>"${work}/index"
    while IFS=$'\t' read -r file heading path; do
        jq -nc --arg file "$file" --arg heading "$heading" --arg sha "$(sha256_file "$path")" --arg path "$path" \
            '{file: $file, heading: $heading, sha: $sha, path: $path}'
    done <"${work}/index"
}

block_file() {
    case "$1" in
        linux_vorpal) echo "sdk/rust/src/artifact/linux_vorpal.rs" ;;
        *) echo "sdk/rust/src/artifact/linux_vorpal/script/$1.rs" ;;
    esac
}

# jq: ids in $toc that are neither mapped nor skipped by the ledger (input).
JQ_UNMAPPED='
    [.book_sections[].id] as $m | .book_skipped as $s |
    $toc[] | select(. as $id |
        ($m | index($id)) == null and
        ($s | any(.id == $id or (.prefix as $p | $p != null and ($id | startswith($p))))) == false)'

# jq: mapped LFS sections that changed place between the order $was (the
# tracked book's contents, or the ledger order) and $toc. Sections on a
# longest run kept in the same relative order did not move; the rest did.
JQ_MOVED='
    [.book_sections[].id | select(startswith("lfs:"))] as $m |
    [$was[] | select(. as $i | ($m | index($i)) and ($toc | index($i)))] as $old |
    [$old[] as $i | $toc | index($i)] as $p |
    ($p | length) as $n |
    if $n == 0 then [] else
        (reduce range(0; $n) as $i ({dp: [], pr: []};
            . as $s |
            ([range(0; $i) | select($p[.] < $p[$i])] | max_by($s.dp[.])) as $b |
            .dp[$i] = (if $b == null then 1 else $s.dp[$b] + 1 end) | .pr[$i] = $b)) as $s |
        ($s.dp | to_entries | max_by(.value) | .key) as $end |
        [$end | recurse(if $s.pr[.] == null then empty else $s.pr[.] end)] as $kept |
        [range(0; $n) | select(. as $i | $kept | index($i) | not) | $old[.]]
    end'

# Prints the stage blocks that implement ID, each with its text.
print_blocks_for() {
    local id="$1" blocks="$2" bfile bhead
    while IFS=$'\t' read -r bfile bhead; do
        echo "--- stage block $(block_file "$bfile"): ${bhead}"
        jq -r --arg f "$bfile" --arg h "$bhead" '.[] | select(.file == $f and .heading == $h) | .path' <<<"$blocks" |
            while IFS= read -r p; do sed 's/^/    /' "$p"; done
    done < <(jq -r --arg id "$id" '.stage_blocks[] | select((.sections // []) | index($id)) | "\(.file)\t\(.heading)"' "$LEDGER")
}

print_snapshot_diff() {
    local id="$1" from="$2" to="$3" old="$4" new="$5"
    if [ -f "$old" ]; then
        tail -n +5 "$old" >"${CACHE}/diff.old"
        tail -n +5 "$new" >"${CACHE}/diff.new"
        diff -u -L "${id} (${from})" -L "${id} (${to})" "${CACHE}/diff.old" "${CACHE}/diff.new" || true
    else
        echo "(no committed snapshot; the commands are new)"
        tail -n +5 "$new"
    fi
}

# Fetches the target book, stages a snapshot of every mapped, id-skipped, and
# new section under DIR/book, and reports each change: a mapped section's
# command diff beside the stage blocks that implement it, a skipped section's
# diff, each new section's commands, removed sections, and mapped sections
# whose order changed.
book_plan() {
    local dir="$1" to="$2" tracked out toc from_toc id old sha rel blocks rc page
    tracked=$(jq -r '.lfs.tracked' "$LEDGER")
    [ "$(jq -r '.blfs.tracked' "$LEDGER")" = "$tracked" ] || die "ledger lfs.tracked and blfs.tracked differ; the book set moves as one"
    [ -n "$to" ] || to=$(lfs_book_stable)
    [ -n "$to" ] || die "cannot read the stable LFS version"
    if [ "$to" != "$tracked" ] &&
        [ "$(printf '%s\n%s\n' "${to%-systemd}" "${tracked%-systemd}" | max_version)" = "${tracked%-systemd}" ]; then
        die "${to} is older than the tracked ${tracked}"
    fi
    out="${dir}/book"
    [ ! "$out" -ef "$BOOK_SNAPSHOTS" ] ||
        die "--dir ${dir} would overwrite the committed snapshots in ${BOOK_SNAPSHOTS}"
    rm -rf "$out"
    mkdir -p "$out"
    log "book: tracked ${tracked}, target ${to}"

    toc=$(lfs_toc "$to" | jq -R . | jq -sc .)
    from_toc=$(lfs_toc "$tracked" | jq -R . | jq -sc .)
    blocks=$(stage_blocks | jq -sc .)
    local changed="" same="" removed="" empty="" schanged="" ssame="" sremoved=""
    while IFS=$'\t' read -r id old; do
        rel=$(snapshot_rel "$id")
        rc=0
        page=$(fetch_section "$id" "$to") || rc=$?
        if [ "$rc" -eq 2 ]; then removed="${removed}${id}"$'\n'; continue; fi
        [ "$rc" -eq 0 ] || exit "$rc"
        sha=$(write_snapshot "$id" "$to" "$page" "${out}/${rel}")
        [ "$sha" != "$EMPTY_SHA" ] || empty="${empty}${id}"$'\n'
        if [ "$sha" = "$old" ]; then same="${same}${id}"$'\n'; continue; fi
        changed="${changed}${id}"$'\n'
        echo "=== ${id}: commands changed"
        print_snapshot_diff "$id" "$tracked" "$to" "${BOOK_SNAPSHOTS}/${rel}" "${out}/${rel}"
        print_blocks_for "$id" "$blocks"
        echo
    done < <(jq -r '.book_sections[] | [.id, (.sha // "")] | @tsv' "$LEDGER")

    while IFS=$'\t' read -r id old; do
        rel=$(snapshot_rel "$id")
        rc=0
        page=$(fetch_section "$id" "$to") || rc=$?
        if [ "$rc" -eq 2 ]; then sremoved="${sremoved}${id}"$'\n'; continue; fi
        [ "$rc" -eq 0 ] || exit "$rc"
        sha=$(write_snapshot "$id" "$to" "$page" "${out}/${rel}")
        if [ "$sha" = "$old" ]; then ssame="${ssame}${id}"$'\n'; continue; fi
        schanged="${schanged}${id}"$'\n'
        echo "=== ${id} (skipped: $(jq -r --arg id "$id" '.book_skipped[] | select(.id == $id) | .reason' "$LEDGER")): commands changed"
        print_snapshot_diff "$id" "$tracked" "$to" "${BOOK_SNAPSHOTS}/${rel}" "${out}/${rel}"
        echo
    done < <(jq -r '.book_skipped[] | select(.id != null) | [.id, (.sha // "")] | @tsv' "$LEDGER")

    local unmapped moved
    unmapped=$(jq -r --argjson toc "$toc" "$JQ_UNMAPPED" "$LEDGER")
    while IFS= read -r id; do
        [ -n "$id" ] || continue
        page=$(fetch_section "$id" "$to") || die "${id} is in the ${to} contents but its page is missing"
        write_snapshot "$id" "$to" "$page" "${out}/$(snapshot_rel "$id")" >/dev/null
        echo "=== ${id}: new section, neither mapped nor skipped"
        tail -n +5 "${out}/$(snapshot_rel "$id")"
        echo
    done <<<"$unmapped"
    moved=$(jq -c --argjson toc "$toc" --argjson was "$from_toc" "$JQ_MOVED" "$LEDGER")
    if [ "$moved" != "[]" ]; then
        echo "=== mapped sections whose place in the book changed"
        jq -r --argjson toc "$toc" '.[] as $i | "  \($i) is now after \(($toc | index($i)) as $n | if $n > 0 then $toc[$n - 1] else "(start)" end)"' <<<"$moved"
        echo
    fi

    jq -n --arg from "$tracked" --arg to "$to" --argjson toc "$toc" --argjson from_toc "$from_toc" \
        --arg changed "$changed" --arg same "$same" --arg removed "$removed" --arg empty "$empty" \
        --arg schanged "$schanged" --arg ssame "$ssame" --arg sremoved "$sremoved" \
        --arg unmapped "$unmapped" --argjson moved "$moved" --slurpfile l "$LEDGER" '
        def lines: split("\n") | map(select(length > 0));
        ($l[0].book_skipped | map(select(.prefix != null) | .prefix)) as $prefixes |
        def prefixed: . as $i | $prefixes | any(. as $p | $i | startswith($p));
        {from: $from, to: $to, changed: ($changed | lines), same: ($same | lines),
         removed: ($removed | lines), empty: ($empty | lines), unmapped: ($unmapped | lines),
         moved: $moved, skipped_changed: ($schanged | lines), skipped_same: ($ssame | lines),
         skipped_removed: ($sremoved | lines),
         prefix_added: [$toc[] | select(prefixed) | select(. as $i | $from_toc | index($i) | not)],
         prefix_dropped: [$from_toc[] | select(prefixed) | select(. as $i | $toc | index($i) | not)],
         toc: $toc}' >"${out}/plan.json"

    jq -r '
        "book-diff \(.from) -> \(.to): \(.changed | length) changed, \(.same | length) unchanged, " +
        "\(.removed | length) removed, \(.unmapped | length) new, \(.moved | length) moved; " +
        "skipped: \(.skipped_changed | length) changed, \(.skipped_removed | length) removed",
        (.changed[] | "  changed          \(.)"),
        (.moved[] | "  moved            \(.)"),
        (.removed[] | "  removed          \(.) (remap its stage blocks with --map, then --unmap it)"),
        (.unmapped[] | "  new              \(.) (implement and --map it, or --skip it with --reason)"),
        (.empty[] | "  empty            \(.) (no commands extracted; inspect the page)"),
        (.skipped_changed[] | "  skipped, changed \(.) (confirm the skip with --skip, or map it)"),
        (.skipped_removed[] | "  skipped, removed \(.) (accept drops its skip)"),
        (.prefix_added[] | "  info: new under a prefix skip: \(.)"),
        (.prefix_dropped[] | "  info: dropped under a prefix skip: \(.)")
    ' "${out}/plan.json"
    log "plan: ${out}/plan.json; snapshots staged under ${out}"
}

# Records the plan's hashes, installs its snapshots, and moves the tracked
# books to its target. A changed or moved section becomes unreconciled with
# changed=true; on a book move, so does every section never audited (the
# first move audits all). A changed skipped section becomes unreconciled.
book_accept() {
    local dir="$1" out plan from to id rel shas="{}"
    out="${dir}/book"
    plan="${out}/plan.json"
    [ -f "$plan" ] || die "no ${plan}; run book-diff first"
    from=$(jq -r '.from' "$plan")
    to=$(jq -r '.to' "$plan")
    [ "$from" = "$(jq -r '.lfs.tracked' "$LEDGER")" ] || die "the plan starts at ${from} but the ledger tracks $(jq -r '.lfs.tracked' "$LEDGER"); run book-diff again"

    local bad
    bad=$(jq -r --argjson toc "$(jq -c '.toc' "$plan")" "$JQ_UNMAPPED" "$LEDGER")
    [ -z "$bad" ] || die "map (--map) or skip (--skip ID --reason R) these sections first:"$'\n'"${bad}"
    bad=$(jq -r '.empty[]' "$plan")
    [ -z "$bad" ] || die "no commands were extracted for:"$'\n'"${bad}"$'\n'"inspect the pages before accepting"
    bad=$(jq -r --slurpfile p "$plan" '
        .stage_blocks[] | select((.sections // []) - $p[0].removed != (.sections // [])) | "\(.file): \(.heading)"' "$LEDGER")
    [ -z "$bad" ] || die "these stage blocks map to sections the target book removed; remap them with --map:"$'\n'"${bad}"
    bad=$(jq -r --slurpfile p "$plan" '
        ($p[0] | .changed + .same + .unmapped + .removed) as $seen |
        (.book_sections[].id | select(. as $i | $seen | index($i) | not)),
        ((.book_skipped[] | .id // empty) | select(. as $i | $p[0] | .skipped_changed + .skipped_same + .skipped_removed + .unmapped | index($i) | not))
    ' "$LEDGER")
    [ -z "$bad" ] || die "mapped or skipped after the plan, so not snapshotted; run book-diff again:"$'\n'"${bad}"

    while IFS= read -r id; do
        [ -n "$id" ] || continue
        rel=$(snapshot_rel "$id")
        shas=$(jq -c --arg id "$id" --arg sha "$(snapshot_sha "${out}/${rel}")" '. + {($id): $sha}' <<<"$shas")
    done < <(jq -r --slurpfile p "$plan" '(.book_sections[].id), (.book_skipped[] | .id // empty) | select(. as $i | $p[0].removed + $p[0].skipped_removed | index($i) | not)' "$LEDGER")

    jq --slurpfile p "$plan" --argjson shas "$shas" '
        $p[0] as $p |
        ($p.from != $p.to) as $moved_book |
        (any(.book_sections[]; .status == "unaudited")) as $baseline_open |
        .book_sections |= (
            map(select(.id as $i | $p.removed | index($i) | not))
            | map(
                .sha as $old | .id as $id |
                (($old != null and $old != $shas[$id]) or ($old == null and ($p.unmapped | index($id)) != null)
                    or ($p.moved | index($id)) != null) as $changed |
                .sha = $shas[$id] |
                if $changed then .status = "unreconciled" | .changed = true
                elif ($moved_book and $baseline_open and .status == "unaudited") then .status = "unreconciled"
                else . end)
            | (map(select(.id | startswith("lfs:"))) | sort_by(.id as $i | $p.toc | index($i))) +
              map(select(.id | startswith("lfs:") | not)))
        | .book_skipped |= (
            map(select((.id // "") as $i | $p.skipped_removed | index($i) | not))
            | map(if .id == null then . else
                .sha as $old | .sha = $shas[.id] |
                if $old != null and $old != .sha then .status = "unreconciled" else . end end))
        | if $moved_book then
            .lfs.tracked = $p.to | .blfs.tracked = $p.to
            | .lfs.book = "https://www.linuxfromscratch.org/lfs/view/\($p.to)/"
            | .blfs.book = "https://www.linuxfromscratch.org/blfs/view/\($p.to)/"
            | .lfs.patches = "https://www.linuxfromscratch.org/patches/lfs/\($p.to | sub("-systemd$"; ""))/"
            | .blfs.patches = "https://www.linuxfromscratch.org/patches/blfs/\($p.to | sub("-systemd$"; ""))/"
          else . end
    ' "$LEDGER" >"${LEDGER}.part" || { rm -f "${LEDGER}.part"; die "accept failed; nothing changed"; }
    mv "${LEDGER}.part" "$LEDGER"

    while IFS= read -r id; do
        rel=$(snapshot_rel "$id")
        mkdir -p "$(dirname "${BOOK_SNAPSHOTS}/${rel}")"
        cp "${out}/${rel}" "${BOOK_SNAPSHOTS}/${rel}"
    done < <(jq -r '(.book_sections[].id), (.book_skipped[] | .id // empty)' "$LEDGER")
    while IFS= read -r id; do
        rm -f "${BOOK_SNAPSHOTS}/$(snapshot_rel "$id")"
    done < <(jq -r '.removed[], .skipped_removed[]' "$plan")
    log "accepted ${to}: $(jq '[.book_sections[], .book_skipped[] | select(.status == "unreconciled")] | length' "$LEDGER") section(s) to reconcile"
}

# Records a reviewed section (or a local block) and refreshes the hashes of
# the stage blocks that implement it. --map FILE:HEADING=IDS|local:REASON maps
# a new or renamed block (IDS comma-separated; a section new to the map is
# added). A block that is gone, or a change in block order, is accepted only
# by a --mark of an affected section (gone) or with --reorder (order).
book_mark() {
    local mark="$1" disp="$2" reason="$3" maps="$4" reorder="$5" cur
    if [ -n "$mark" ] && [ "${mark#local:}" = "$mark" ]; then
        case "$disp" in
            edited | unchanged) ;;
            deviation) [ -n "$reason" ] || die "a deviation needs --reason" ;;
            *) die "--mark SECTION needs --disposition edited|unchanged|deviation" ;;
        esac
    fi

    local spec file rest heading target mapjson="[]"
    while IFS= read -r spec; do
        [ -n "$spec" ] || continue
        file=${spec%%:*}
        rest=${spec#*:}
        heading=${rest%%=*}
        target=${rest#*=}
        [ "$file" != "$spec" ] && [ "$heading" != "$rest" ] && [ -n "$target" ] ||
            die "--map takes FILE:HEADING=SECTION[,SECTION] or FILE:HEADING=local:REASON"
        mapjson=$(jq -c --arg f "$file" --arg h "$heading" --arg t "$target" '
            . + [{file: $f, heading: $h} + (if ($t | startswith("local:"))
                then {local: ($t | ltrimstr("local:"))} else {sections: ($t | split(","))} end)]' <<<"$mapjson")
    done <<<"$maps"

    cur=$(stage_blocks | jq -sc 'map(del(.path))')
    jq --argjson cur "$cur" --argjson maps "$mapjson" --arg mark "$mark" \
        --arg disp "$disp" --arg reason "$reason" --argjson reorder "$reorder" '
        def key: "\(.file): \(.heading)";
        ([$cur[] | key] | group_by(.) | map(select(length > 1) | .[0])) as $dups |
        if ($dups | length) > 0 then error("duplicate stage block headings: \($dups)") else . end |
        ($maps | map(select(key as $k | $cur | any(key == $k) | not) | key)) as $nomatch |
        if ($nomatch | length) > 0 then error("--map names no current stage block: \($nomatch)") else . end |

        ($maps | map(.sections // []) | add // [] | unique | map(select(. as $i | $l0[0].book_sections | any(.id == $i) | not))) as $new |
        ($new | map(select(test("^(lfs:chapter0[4-8]|blfs:[a-z0-9-]+)/[A-Za-z0-9+._-]+$") | not))) as $bad |
        if ($bad | length) > 0 then error("--map: not a book section id: \($bad)") else . end |
        # A section new to the map is dropped from book_skipped and starts
        # unreconciled; book-diff snapshots it and --accept records it.
        .book_skipped |= map(select((.id // "") as $i | $new | index($i) | not)) |
        .book_sections += [$new[] | {id: ., sha: null, status: "unreconciled", disposition: null, reason: null}] |
        . as $l |

        if $mark != "" and ($mark | startswith("local:") | not) and ($l.book_sections | any(.id == $mark) | not)
        then error("\($mark) is not a mapped book section") else . end |
        # A local mark names a block the ledger records as local (current or
        # gone), or one this call maps to local.
        def lkey($b): "local:\($b.file):\($b.heading)";
        if ($mark | startswith("local:")) and
            ([$l.stage_blocks[] | select(lkey(.) == $mark and ((.local // "") != ""))] | length) == 0 and
            ([$maps[] | select(lkey(.) == $mark and .local != null)] | length) == 0
        then error("\($mark) names no local stage block; a block mapped to sections is marked through a section") else . end |

        def refs($b): (($b.sections // []) | index($mark)) != null or ($mark == lkey($b) and (($b.local // "") != ""));
        def old($b): [$l.stage_blocks[] | select(.file == $b.file and .heading == $b.heading)] | first;
        def mapped($b): [$maps[] | select(.file == $b.file and .heading == $b.heading)] | first;

        ([$l.stage_blocks[] | select(key as $k | $cur | any(key == $k) | not)]) as $gone |
        ($gone | map(select(refs(.) | not) | key)) as $unacked |
        if ($unacked | length) > 0
        then error("stage blocks are gone: \($unacked); --mark one of their sections to record the removal")
        else . end |
        ([$cur[] | key | select(. as $k | $l.stage_blocks | any(key == $k))]) as $cur_order |
        ([$l.stage_blocks[] | key | select(. as $k | $cur | any(key == $k))]) as $old_order |
        if $cur_order != $old_order and ($reorder | not)
        then error("stage blocks were reordered; review the order against the book, then pass --reorder")
        else . end |

        # Blocks of the marked section, after this call: changed when new,
        # remapped onto it, or edited since their last mark.
        ([$cur[] as $b | (mapped($b)) as $m | (old($b)) as $o |
            select(if $m != null then ((($m.sections // []) | index($mark)) != null or ($o != null and refs($o))) else ($o != null and refs($o)) end) |
            select($o == null or $o.sha != $b.sha or $m != null)] | length
          + ([$gone[] | select(refs(.))] | length)) as $changed_blocks |
        if $mark != "" and ($mark | startswith("local:") | not) then
            if $disp == "edited" and $changed_blocks == 0
            then error("no stage block of \($mark) changed since its last mark; use --disposition unchanged or deviation")
            elif $disp == "unchanged" and $changed_blocks > 0
            then error("a stage block of \($mark) changed; use --disposition edited")
            elif $disp == "unchanged" and ([$l.book_sections[] | select(.id == $mark) | .changed // false] | first) and $reason == ""
            then error("the book changed \($mark); --disposition unchanged needs --reason saying why the blocks already match")
            else . end
        else . end |

        .stage_blocks = [
            $cur[] as $b | (old($b)) as $o | (mapped($b)) as $m |
            if $m != null then
                # A block mapped onto sections without a mark of one of them
                # keeps no hash, so it reads as changed until the review of
                # one of its sections marks it. A local map records its reason as the review.
                {file: $b.file, heading: $b.heading,
                 sha: (if (($m.sections // []) | index($mark)) != null then $b.sha
                       elif $m.local != null then $b.sha
                       else null end)}
                + ($m | del(.file, .heading))
            elif $o == null then error("new stage block \($b.file): \($b.heading); map it with --map")
            elif refs($o) then $o + {sha: $b.sha}
            else $o end
        ] |
        if $mark != "" and ($mark | startswith("local:") | not) then
            .book_sections |= map(if .id == $mark then
                (if .status == "unaudited" then . else .status = "audited" end)
                | .disposition = $disp
                | .reason = (if $reason == "" then null else $reason end)
                | del(.changed)
                else . end)
        else . end
    ' --slurpfile l0 "$LEDGER" "$LEDGER" >"${LEDGER}.part" 2>"${CACHE}/mark.err" || {
        rm -f "${LEDGER}.part"
        sed 's/^jq: error (at [^)]*): //' "${CACHE}/mark.err" >&2
        die "nothing recorded"
    }
    mv "${LEDGER}.part" "$LEDGER"
    [ -z "$mark" ] || log "marked ${mark}${disp:+ (${disp})}"
}

book_skip() {
    local id="$1" reason="$2"
    [ -n "$reason" ] || die "--skip needs --reason"
    jq -e --arg id "$id" 'any(.book_sections[]; .id == $id) | not' "$LEDGER" >/dev/null ||
        die "${id} is mapped; remap its stage blocks, then --unmap it before skipping it"
    jq --arg id "$id" --arg r "$reason" '
        if any(.book_skipped[]; .id == $id)
        then .book_skipped |= map(if .id == $id then .reason = $r | del(.status) else . end)
        else .book_skipped += [{id: $id, reason: $r, sha: null}] end' "$LEDGER" >"${LEDGER}.part"
    mv "${LEDGER}.part" "$LEDGER"
    log "skipped ${id}; a new skip needs book-diff and --accept to snapshot it"
}

book_unmap() {
    local id="$1"
    jq -e --arg id "$id" 'any(.book_sections[]; .id == $id)' "$LEDGER" >/dev/null || die "${id} is not mapped"
    jq --arg id "$id" '
        .book_sections |= map(select(.id != $id))
        | .stage_blocks |= map(if .sections then .sections -= [$id] else . end)' "$LEDGER" >"${LEDGER}.part"
    mv "${LEDGER}.part" "$LEDGER"
    rm -f "${BOOK_SNAPSHOTS}/$(snapshot_rel "$id")"
    log "unmapped ${id}; --skip it with a reason unless the book dropped it"
}

book_show() {
    local id="$1" rel
    rel=$(snapshot_rel "$id")
    jq -r --arg id "$id" '(.book_sections[], .book_skipped[]) | select(.id == $id) |
        "\(.id): status \(.status // "skipped"), disposition \(.disposition // "-"), reason \(.reason // "-")"' "$LEDGER"
    if [ -f "${BOOK_SNAPSHOTS}/${rel}" ]; then cat "${BOOK_SNAPSHOTS}/${rel}"; else echo "(no snapshot)"; fi
    echo
    print_blocks_for "$id" "$(stage_blocks | jq -sc .)"
}

# Fails when the stage scripts, the ledger, the committed snapshots, or (unless
# --offline) the tracked book disagree.
book_check() {
    local offline="$1" tracked cur problems id rel sha want toc rc page
    tracked=$(jq -r '.lfs.tracked' "$LEDGER")
    cur=$(stage_blocks | jq -sc 'map(del(.path))')
    problems=$(jq -r --argjson cur "$cur" '
        . as $l |
        def key: "\(.file): \(.heading)";
        (if .lfs.tracked != .blfs.tracked then "lfs.tracked and blfs.tracked differ" else empty end),
        ([$cur[] | key] | group_by(.) | map(select(length > 1) | "duplicate stage block heading \(.[0])")[]),
        ([$cur[] | key] - [$l.stage_blocks[] | key] | .[] | "new stage block \(.) (map it with --map)"),
        ([$l.stage_blocks[] | key] - [$cur[] | key] | .[] | "stage block \(.) is gone (--mark one of its sections, or --mark local:FILE:HEADING for a local block)"),
        (if ([$cur[] | key | select(. as $k | $l.stage_blocks | any(key == $k))]) !=
            ([$l.stage_blocks[] | key | select(. as $k | $cur | any(key == $k))])
            then "stage blocks were reordered (review the order, then --mark with --reorder)" else empty end),
        ($l.stage_blocks[] as $o | $cur[] | select(key == ($o | key) and .sha != $o.sha) |
            "\(key) changed since its last mark (book-diff --mark \(($o.sections // ["local:\($o.file):\($o.heading)"]) | join(" or ")))"),
        ($l.stage_blocks[] | select(((.sections // []) | length) == 0 and ((.local // "") | length) == 0) | "\(key) maps to no book section and has no local reason"),
        ($l.stage_blocks[] | .sections // [] | .[] | select(. as $i | $l.book_sections | any(.id == $i) | not) | "stage blocks name unmapped section \(.)"),
        ($l.book_sections[] | select(.id as $i | $l.stage_blocks | any((.sections // []) | index($i)) | not) | "\(.id) maps to no stage block"),
        ($l.book_sections[] | select(.status == "unreconciled") | "\(.id): unreconciled; review its stage blocks against script/source/book, then --mark"),
        ($l.book_skipped[] | select(.status == "unreconciled") | "\(.id): skipped section changed; confirm with --skip or map it"),
        ($l.book_sections[] | select(.disposition == "deviation" and ((.reason // "") | length) == 0) | "\(.id): deviation without a reason"),
        ([$l.book_sections[].id] as $m | $l.book_skipped[] | select(.id != null) | select(.id as $i | $m | index($i)) | "\(.id) is both mapped and skipped")
    ' "$LEDGER")

    while IFS=$'\t' read -r id want; do
        rel=$(snapshot_rel "$id")
        if [ ! -f "${BOOK_SNAPSHOTS}/${rel}" ]; then
            problems="${problems}"$'\n'"${id}: no snapshot at script/source/book/${rel} (book-diff --to ${tracked}, then --accept)"
            continue
        fi
        sha=$(snapshot_sha "${BOOK_SNAPSHOTS}/${rel}")
        [ "$sha" = "$want" ] || problems="${problems}"$'\n'"${id}: snapshot does not match the ledger sha"
        head -1 "${BOOK_SNAPSHOTS}/${rel}" | grep -q " ${tracked}: " ||
            problems="${problems}"$'\n'"${id}: snapshot is not from ${tracked}"
    done < <(jq -r '(.book_sections[], .book_skipped[]) | select(.id != null) | [.id, (.sha // "")] | @tsv' "$LEDGER")

    if [ "$offline" != 1 ]; then
        toc=$(lfs_toc "$tracked" | jq -R . | jq -sc .)
        problems="${problems}"$'\n'"$(jq -r --argjson toc "$toc" "$JQ_UNMAPPED"' | "\(.) is in the book but neither mapped nor skipped"' "$LEDGER")"
        problems="${problems}"$'\n'"$(jq -r --argjson toc "$toc" --argjson was "$(jq -c "[.book_sections[].id]" "$LEDGER")" "${JQ_MOVED}"' | if length > 0 then "mapped LFS sections are out of book order: \(join(", "))" else empty end' "$LEDGER")"
        problems="${problems}"$'\n'"$(jq -r --argjson toc "$toc" '(.book_sections[].id, (.book_skipped[] | .id // empty)) | select(startswith("lfs:")) | select(. as $i | $toc | index($i) | not) | "\(.) is not in the \($toc | length)-section book"' "$LEDGER")"
        while IFS=$'\t' read -r id want; do
            rc=0
            page=$(fetch_section "$id" "$tracked") || rc=$?
            if [ "$rc" -ne 0 ]; then
                problems="${problems}"$'\n'"${id}: no ${tracked} page"
                continue
            fi
            book_commands "$page" >"${CACHE}/check.body"
            [ "$(sha256_file "${CACHE}/check.body")" = "$want" ] ||
                problems="${problems}"$'\n'"${id}: the ${tracked} page no longer matches its snapshot (book errata or extractor change; book-diff --to ${tracked}, then --accept)"
        done < <(jq -r '(.book_sections[], .book_skipped[]) | select(.id != null) | [.id, (.sha // "")] | @tsv' "$LEDGER")
    fi

    problems=$(printf '%s\n' "$problems" | grep -v '^$' || true)
    local unaudited
    unaudited=$(jq '[.book_sections[] | select(.status == "unaudited")] | length' "$LEDGER")
    [ "$unaudited" -eq 0 ] || log "note: ${unaudited} section(s) never audited; the next book move audits them all"
    if [ -n "$problems" ]; then
        printf '%s\n' "$problems" >&2
        die "book-diff --check: $(printf '%s\n' "$problems" | wc -l | tr -d ' ') problem(s)"
    fi
    if [ "$offline" = 1 ]; then
        log "book-diff --check: stage scripts, ledger, and snapshots agree (offline)"
    else
        log "book-diff --check: stage scripts, ledger, snapshots, and the ${tracked} book agree"
    fi
}

cmd_book_diff() {
    local mode=plan to="" dir="$STAGE_DIR" offline=0 mark="" disp="" reason="" id="" maps="" reorder=false
    while [ $# -gt 0 ]; do
        case "$1" in
            --to) to="$2"; shift 2 ;;
            --dir) dir="$2"; shift 2 ;;
            --check) mode=check; shift ;;
            --offline) offline=1; shift ;;
            --accept) mode=accept; shift ;;
            --mark) mode=mark; mark="$2"; shift 2 ;;
            --map) maps="${maps}$2"$'\n'; shift 2 ;;
            --reorder) reorder=true; shift ;;
            --disposition) disp="$2"; shift 2 ;;
            --reason) reason="$2"; shift 2 ;;
            --skip) mode=skip; id="$2"; shift 2 ;;
            --unmap) mode=unmap; id="$2"; shift 2 ;;
            --show) mode=show; id="$2"; shift 2 ;;
            *) die "unknown book-diff option: $1" ;;
        esac
    done
    case "$dir" in /*) ;; *) dir="${PWD}/${dir}" ;; esac
    if [ -n "$maps" ] || [ "$reorder" = true ]; then
        case "$mode" in
            plan) mode=mark ;;
            mark) ;;
            *) die "--map and --reorder go with --mark or alone" ;;
        esac
    fi

    case "$mode" in
        plan) book_plan "$dir" "$to" ;;
        accept) book_accept "$dir" ;;
        mark) book_mark "$mark" "$disp" "$reason" "$maps" "$reorder" ;;
        skip) book_skip "$id" "$reason" ;;
        unmap) book_unmap "$id" ;;
        show) book_show "$id" ;;
        check) book_check "$offline" ;;
    esac
}

main() {
    CACHE=$(mktemp -d)
    trap 'rm -rf "$CACHE"' EXIT

    local cmd="${1:-lock}"
    [ $# -gt 0 ] && shift

    case "$cmd" in
        lock) cmd_lock "$@" ;;
        check) cmd_check "$@" ;;
        stage) cmd_stage "$@" ;;
        mirror-check) cmd_mirror_check "$@" ;;
        upload-cmd) cmd_upload_cmd "$@" ;;
        provenance) cmd_provenance "$@" ;;
        pins) cmd_pins "$@" ;;
        set-version) cmd_set_version "$@" ;;
        digest) cmd_digest "$@" ;;
        digest-fill) cmd_digest_fill "$@" ;;
        book-diff) cmd_book_diff "$@" ;;
        -h | --help | help) sed -n '7,80p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' ;;
        *) die "unknown command: ${cmd} (see --help)" ;;
    esac
}

main "$@"
