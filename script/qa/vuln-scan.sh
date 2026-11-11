#!/usr/bin/env bash
#
# vuln-scan — known-vulnerability scan of every tracked lockfile.
#
# A hash-pinned lockfile buys integrity, not vulnerability freedom. This gate
# asks whether a dependency this tree carries, or one a step just added, has a
# published advisory. Adapted from the docket corpus's Cargo.lock gate
# (dotfiles .docket/bin/vuln-scan) and extended to this repository's lockfiles.
#
# WHAT IT QUERIES. Every dependency in each tracked Cargo.lock, go.sum, uv.lock
# and bun.lock (the SDKs, the website and the `vorpal init` templates, which
# ship to user projects) is looked up by name and exact version against OSV
# (https://osv.dev) with curl and jq. Version matching happens server-side
# against each advisory's own ranges, so this script parses no semver.
#
# VERDICTS.
#   - an advisory with no `informational` marker is a vulnerability: exit 1;
#   - an informational advisory (RustSec unmaintained, unsound, notice) warns,
#     exit 0, since failing on it would block every step after a transitive
#     dependency goes unmaintained;
#   - withdrawn advisories are ignored;
#   - anything that stops the scan from completing exits 2, labelled
#     INFRASTRUCTURE: OSV unreachable after retries, answering HTTP 4xx, or
#     answering in a shape this script cannot read, a paginated result, a
#     tracked lockfile of a kind this script does not parse, a lockfile that
#     parses to nothing, or a canary that fails.
#     A scan that did not run has scanned nothing, so it is never a pass.
#
# CANARIES. Each run also queries one known-vulnerable version per ecosystem
# present and exits 2 unless OSV flags it. A wrong ecosystem name or version
# form otherwise returns an empty, clean-looking result.
#
# RANGE SCOPING. With DOCKET_GATE_BASE set, only an entry the range added or
# changed can fail: an entry is pre-existing when the same lockfile at the base
# commit holds the same ecosystem, name, version and source. Pre-existing
# matches are printed, marked as such. A lockfile absent at the base is an empty
# baseline. With DOCKET_GATE_BASE unset or empty, every lockfile entry is in
# scope. A base that does not resolve to a commit exits 2.
#
# NOT QUERIED, and stated rather than implied by silence: entries OSV cannot
# match by name and version are counted and listed as unqueried (Cargo path
# packages, uv editable or virtual packages, bun workspace, git, file and alias
# entries). Vorpal.lock pins tool artifacts and no advisory database indexes
# them.
#
# There is no ignore list, on purpose: an advisory ruled acceptable is recorded
# on the issue, where a reviewer sees it.
#
# Portable to bash 3.2 and BWK awk: no associative arrays, mapfile or sort -V.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
if ! repo_root="$(git -C "$script_dir" rev-parse --show-toplevel 2>/dev/null)"; then
  echo "vuln-scan FAILED: $script_dir is not inside a git repository." >&2
  exit 1
fi
cd "$repo_root"

readonly OSV_HOST="api.osv.dev"
readonly OSV_BATCH_URL="https://${OSV_HOST}/v1/querybatch"
readonly OSV_VULN_URL="https://${OSV_HOST}/v1/vulns"
readonly BATCH_SIZE=500
readonly OSV_ATTEMPTS=4

for tool in curl jq git awk; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "vuln-scan FAILED: $tool is required and is not on PATH." >&2
    exit 1
  fi
done

work="$(mktemp -d "${TMPDIR:-/tmp}/vuln-scan.XXXXXX")"
trap 'rm -rf "$work"' EXIT

infra() {
  {
    echo
    echo "vuln-scan FAILED: INFRASTRUCTURE — $1"
    echo "No complete scan ran, so nothing is known about vulnerabilities either way."
    echo "Do NOT go hunting for a finding: fix the cause and re-run the gate."
  } >&2
  exit 2
}

# osv_get <url> <out> <curl args...>: write the body of an HTTP 200 answer from
# <url> to <out>. A curl error or HTTP 5xx is retried up to OSV_ATTEMPTS
# attempts with a doubling delay; any other status fails at once, since
# repeating a request OSV rejected cannot change its answer.
osv_get() {
  local url="$1" out="$2" attempt=1 delay=2 code failure
  shift 2
  while :; do
    if code="$(curl -sS -o "$out" -w '%{http_code}' "$@" "$url" 2>"$work/curl.err")"; then
      case "$code" in
        200) return 0 ;;
        5??) failure="HTTP ${code}" ;;
        *) infra "${url} answered HTTP ${code} on attempt ${attempt} of ${OSV_ATTEMPTS}: $(head -c 300 "$out" 2>/dev/null | tr '\n' ' ')" ;;
      esac
    else
      failure="$(tr '\n' ' ' <"$work/curl.err")"
    fi
    if [ "$attempt" -ge "$OSV_ATTEMPTS" ]; then
      infra "could not get an answer from ${url} after ${attempt} attempts; last failure: ${failure}"
    fi
    echo "vuln-scan: ${url} attempt ${attempt} of ${OSV_ATTEMPTS} failed (${failure}); retrying in ${delay}s" >&2
    sleep "$delay"
    attempt=$((attempt + 1))
    delay=$((delay * 2))
  done
}

# Base commit, if any.
base_sha=""
if [ -n "${DOCKET_GATE_BASE:-}" ]; then
  if ! base_sha="$(git rev-parse --verify --quiet --end-of-options "${DOCKET_GATE_BASE}^{commit}")"; then
    infra "DOCKET_GATE_BASE '${DOCKET_GATE_BASE}' does not resolve to a commit"
  fi
fi

# Parsers: each prints "ecosystem<TAB>name<TAB>version<TAB>source". Ecosystem
# SKIP marks an entry OSV cannot match by name and version.

parse_cargo() {
  awk -F'"' '
    function flush() {
      if (name != "") printf "%s\t%s\t%s\t%s\n", (source == "" ? "SKIP" : "crates.io"), name, version, (source == "" ? "path package" : source)
      name = ""; version = ""; source = ""
    }
    /^\[\[package\]\]/      { flush(); in_pkg = 1; next }
    /^\[/                   { flush(); in_pkg = 0; next }
    in_pkg && /^name = /    { name = $2 }
    in_pkg && /^version = / { version = $2 }
    in_pkg && /^source = /  { source = $2 }
    END { flush() }
  ' "$1"
}

# go.sum lists a module's code hash and its go.mod hash on separate lines; only
# the code line names a version whose code is built.
parse_gosum() {
  awk '$2 !~ /\/go\.mod$/ && NF >= 2 { printf "Go\t%s\t%s\t\n", $1, $2 }' "$1" | sort -u
}

# uv.lock: one [[package]] table per package, keys at column zero.
parse_uv() {
  awk -F'"' '
    function flush() {
      if (name != "") {
        if (source ~ /registry = /) printf "PyPI\t%s\t%s\t%s\n", name, version, "registry"
        else printf "SKIP\t%s\t%s\t%s\n", name, version, "non-registry source"
      }
      name = ""; version = ""; source = ""
    }
    /^\[\[package\]\]/      { flush(); in_pkg = 1; next }
    /^\[/                   { flush(); in_pkg = 0; next }
    in_pkg && /^name = /    { name = $2 }
    in_pkg && /^version = / { version = $2 }
    in_pkg && /^source = /  { source = $0 }
    END { flush() }
  ' "$1"
}

# bun.lock is JSONC, not JSON (trailing commas). Package lines are
# `    "key": ["name@version", ...`; split the first string at its last `@`.
parse_bun() {
  awk -F'"' '
    /^    "[^"]+": \["/ {
      spec = $4
      n = length(spec); at = 0
      for (i = n; i > 1; i--) if (substr(spec, i, 1) == "@") { at = i; break }
      if (at == 0) { printf "SKIP\t%s\t\tno version\n", spec; next }
      name = substr(spec, 1, at - 1); version = substr(spec, at + 1)
      if (version ~ /^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.+-]+)?$/) printf "npm\t%s\t%s\t\n", name, version
      else printf "SKIP\t%s\t%s\t%s\n", name, version, "non-registry or alias"
    }
  ' "$1"
}

parse_file() { # <kind> <file>
  case "$1" in
    Cargo.lock) parse_cargo "$2" ;;
    go.sum) parse_gosum "$2" ;;
    uv.lock) parse_uv "$2" ;;
    bun.lock) parse_bun "$2" ;;
  esac
}

# Enumerate tracked lockfiles. A lockfile kind this script does not parse fails
# closed: it would otherwise sit unscanned behind a clean verdict.
git ls-files >"$work/tracked.txt" || infra "git ls-files failed"
: >"$work/lockfiles.txt"
while IFS= read -r path; do
  base="${path##*/}"
  case "$base" in
    Cargo.lock|go.sum|uv.lock|bun.lock) printf '%s\n' "$path" >>"$work/lockfiles.txt" ;;
    package-lock.json|npm-shrinkwrap.json|yarn.lock|pnpm-lock.yaml|bun.lockb|poetry.lock|Pipfile.lock|Gemfile.lock|composer.lock|packages.lock.json)
      infra "tracked lockfile '$path' is of a kind this script does not parse" ;;
    Vorpal.lock) echo "vuln-scan: $path pins tool artifacts; no advisory database indexes them, so it is out of reach" ;;
  esac
done <"$work/tracked.txt"

if [ ! -s "$work/lockfiles.txt" ]; then
  infra "no tracked lockfile found; refusing to report clean over an empty scan"
fi

# all.tsv: path, ecosystem, name, version, source.
: >"$work/all.tsv"
: >"$work/base.tsv"
while IFS= read -r path; do
  kind="${path##*/}"
  parse_file "$kind" "$path" | awk -v p="$path" '{ print p "\t" $0 }' >"$work/one.tsv" ||
    infra "could not parse $path"
  queryable="$(awk -F'\t' '$2 != "SKIP"' "$work/one.tsv" | wc -l | tr -d ' ')"
  if [ "$queryable" -eq 0 ]; then
    infra "$path parsed to zero queryable dependencies; the parser no longer reads its format"
  fi
  cat "$work/one.tsv" >>"$work/all.tsv"
  if [ -n "$base_sha" ] && git cat-file -e "${base_sha}:${path}" 2>/dev/null; then
    git show "${base_sha}:${path}" >"$work/base.lock" || infra "could not read $path at DOCKET_GATE_BASE ${base_sha}"
    parse_file "$kind" "$work/base.lock" | awk -v p="$path" '{ print p "\t" $0 }' >>"$work/base.tsv"
  fi
done <"$work/lockfiles.txt"

awk -F'\t' '$2 != "SKIP"' "$work/all.tsv" >"$work/deps.tsv"
awk -F'\t' '$2 == "SKIP"' "$work/all.tsv" >"$work/skipped.tsv"
# added.tsv: entries absent from the baseline; with no base, all of them.
# FILENAME, not NR==FNR, marks the first file: an empty base.tsv would
# otherwise load deps.tsv as the baseline and pass everything.
awk -F'\t' 'FILENAME == ARGV[1] { base[$0]; next } !($0 in base)' \
  "$work/base.tsv" "$work/deps.tsv" >"$work/added.tsv"

# real_keys.tsv: unique "ecosystem name version" the tree carries.
cut -f2-4 "$work/deps.tsv" | sort -u >"$work/real_keys.tsv"
real_count="$(wc -l <"$work/real_keys.tsv" | tr -d ' ')"

# Canaries, one per ecosystem present.
: >"$work/canaries.tsv"
for eco in crates.io Go PyPI npm; do
  if awk -F'\t' -v e="$eco" '$1 == e { f = 1; exit } END { exit !f }' "$work/real_keys.tsv"; then
    case "$eco" in
      crates.io) printf 'crates.io\ttime\t0.1.43\n' ;;
      Go) printf 'Go\tgolang.org/x/text\tv0.3.0\n' ;;
      PyPI) printf 'PyPI\trequests\t2.5.0\n' ;;
      npm) printf 'npm\tlodash\t4.17.15\n' ;;
    esac >>"$work/canaries.tsv"
  fi
done

cat "$work/real_keys.tsv" "$work/canaries.tsv" | sort -u >"$work/queries.tsv"
query_count="$(wc -l <"$work/queries.tsv" | tr -d ' ')"

echo "vuln-scan: $(wc -l <"$work/lockfiles.txt" | tr -d ' ') lockfile(s), ${real_count} distinct dependency version(s) to check against OSV"
if [ -n "$base_sha" ]; then
  echo "vuln-scan: scoped to DOCKET_GATE_BASE ${base_sha}: $(wc -l <"$work/added.tsv" | tr -d ' ') entr(ies) added or changed; only those can fail"
else
  echo "vuln-scan: DOCKET_GATE_BASE unset; every lockfile entry is in scope"
fi
skip_count="$(wc -l <"$work/skipped.tsv" | tr -d ' ')"
if [ "$skip_count" -gt 0 ]; then
  echo "vuln-scan: ${skip_count} entr(ies) not queried (OSV cannot match them by name and version):"
  awk -F'\t' '{ printf "  %s: %s %s (%s)\n", $1, $3, $4, $5 }' "$work/skipped.tsv" | sort -u | head -20
fi

jq -Rn '[inputs | split("\t") | {package: {name: .[1], ecosystem: .[0]}, version: .[2]}]' \
  "$work/queries.tsv" >"$work/queries.json"

# hits.tsv: ecosystem, name, version, advisory id.
: >"$work/hits.tsv"
offset=0
while [ "$offset" -lt "$query_count" ]; do
  chunk="$work/chunk_${offset}.json"
  resp="$work/resp_${offset}.json"
  jq -c --argjson s "$offset" --argjson n "$BATCH_SIZE" '{queries: .[$s:$s+$n]}' \
    "$work/queries.json" >"$chunk"
  chunk_len="$(jq '.queries | length' "$chunk")"

  osv_get "$OSV_BATCH_URL" "$resp" --max-time 90 \
    -H 'Content-Type: application/json' -H 'Accept: application/json' \
    --data-binary @"$chunk"
  # One result per query, in order, ties a hit back to its dependency.
  jq -e --argjson n "$chunk_len" \
    '(.results | type) == "array" and (.results | length) == $n' "$resp" >/dev/null ||
    infra "${OSV_BATCH_URL} answered in a shape this script cannot read (expected ${chunk_len} results)"
  # A paginated result would silently truncate; refuse instead.
  jq -e '[.results[] | select(.next_page_token != null)] | length == 0' "$resp" >/dev/null ||
    infra "${OSV_BATCH_URL} paginated a result and this script does not page; refusing to truncate"

  jq -r --slurpfile q "$chunk" '
    [$q[0].queries, .results] | transpose[]
    | .[0] as $query | (.[1].vulns // [])[]
    | [$query.package.ecosystem, $query.package.name, $query.version, .id] | @tsv' \
    "$resp" >>"$work/hits.tsv"

  offset=$((offset + BATCH_SIZE))
done

# Every canary must be flagged, or the query form is wrong.
while IFS=$'\t' read -r eco name version; do
  [ -n "$eco" ] || continue
  if ! awk -F'\t' -v e="$eco" -v n="$name" -v v="$version" \
    '$1 == e && $2 == n && $3 == v { f = 1; exit } END { exit !f }' "$work/hits.tsv"; then
    infra "canary ${eco} ${name} ${version} was not flagged by OSV; the query form for ${eco} is wrong or OSV is degraded"
  fi
done <"$work/canaries.tsv"

# Keep only hits on dependencies the tree carries.
awk -F'\t' 'FILENAME == ARGV[1] { real[$1 "\t" $2 "\t" $3]; next } ($1 "\t" $2 "\t" $3) in real' \
  "$work/real_keys.tsv" "$work/hits.tsv" | sort -u >"$work/hits.real.tsv"

if [ ! -s "$work/hits.real.tsv" ]; then
  echo "vuln-scan: clean — no advisory matches any of the ${real_count} dependency version(s)"
  exit 0
fi

# Fetch each advisory once, RUSTSEC ids first, so the GHSA mirror of an
# advisory already printed folds into that row through its aliases.
awk -F'\t' '{ print $4 }' "$work/hits.real.tsv" | sort -u |
  awk '/^RUSTSEC-/ { print "0\t" $0; next } { print "1\t" $0 }' | sort | cut -f2 >"$work/ids.txt"

vulns=0
preexisting=0
warnings=0
seen_ids=""
echo
while IFS= read -r id; do
  [ -n "$id" ] || continue
  if printf '%s\n' "$seen_ids" | grep -qxF "$id"; then
    continue
  fi

  detail="$work/vuln_${id}.json"
  osv_get "${OSV_VULN_URL}/${id}" "$detail" --max-time 60
  jq -e --arg id "$id" '.id == $id' "$detail" >/dev/null ||
    infra "advisory ${id} came back in a shape this script cannot read"

  aliases="$(jq -r '(.aliases // [])[]' "$detail")"
  seen_ids="$(printf '%s\n%s\n%s' "$seen_ids" "$id" "$aliases")"
  alias_list="$(printf '%s' "$aliases" | tr '\n' ',' | sed 's/,$//; s/,/, /g')"
  summary="$(jq -r '.summary // ((.details // "") | split("\n")[0])' "$detail")"
  withdrawn="$(jq -r '.withdrawn // ""' "$detail")"

  while IFS=$'\t' read -r eco name version hit_id; do
    [ "$hit_id" = "$id" ] || continue
    # RustSec marks unmaintained/unsound/notice advisories per affected package;
    # anything without the marker (every GHSA record included) is a vulnerability.
    kind="$(jq -r --arg name "$name" --arg eco "$eco" '
      [ .affected[]? | select(.package.ecosystem == $eco and .package.name == $name)
        | .database_specific.informational // empty ] | first // "vulnerability"' "$detail")"
    if [ -n "$withdrawn" ]; then
      kind="withdrawn"
    fi
    if [ "$kind" = vulnerability ] && ! awk -F'\t' -v e="$eco" -v n="$name" -v v="$version" \
      '$2 == e && $3 == n && $4 == v { f = 1; exit } END { exit !f }' "$work/added.tsv"; then
      kind="vulnerability, pre-existing at DOCKET_GATE_BASE"
    fi
    where="$(awk -F'\t' -v e="$eco" -v n="$name" -v v="$version" \
      '$2 == e && $3 == n && $4 == v { print $1 }' "$work/deps.tsv" | sort -u | head -3 | tr '\n' ' ')"
    printf '  %-22s %s %s %s  %s  %s  in: %s' "$id" "$eco" "$name" "$version" "$kind" "$summary" "$where"
    if [ -n "$alias_list" ]; then
      printf '  [%s]' "$alias_list"
    fi
    echo
    case "$kind" in
      vulnerability) vulns=$((vulns + 1)) ;;
      vulnerability,*) preexisting=$((preexisting + 1)) ;;
      withdrawn) ;;
      *) warnings=$((warnings + 1)) ;;
    esac
  done <"$work/hits.real.tsv"
done <"$work/ids.txt"

echo
if [ "$vulns" -gt 0 ]; then
  scope="in a tracked lockfile"
  if [ -n "$base_sha" ]; then
    scope="on entr(ies) added or changed since DOCKET_GATE_BASE ${base_sha}"
  fi
  cat >&2 <<EOF
vuln-scan FAILED: ${vulns} vulnerability match(es) ${scope}.

Each row above names the dependency, the version the lockfile pins, the
advisory and the lockfile. Move past the advisory's fixed version, or, where the
issue rules the exposure acceptable, record that ruling on the issue. This gate
holds no ignore list on purpose.
EOF
  exit 1
fi

if [ "$preexisting" -gt 0 ]; then
  echo "vuln-scan: ok — ${preexisting} vulnerability match(es) above on entries already pinned at DOCKET_GATE_BASE ${base_sha}; none on an entry this range added or changed"
fi
if [ "$warnings" -gt 0 ]; then
  echo "vuln-scan: ok — ${warnings} informational advisory(ies) above (unmaintained, unsound or notice); none is a vulnerability"
fi
if [ $((preexisting + warnings)) -eq 0 ]; then
  echo "vuln-scan: clean — the only matches were withdrawn advisories"
fi
exit 0
