#!/bin/bash
set -euo pipefail

# =============================================================================
# Vorpal Source Downloader
# =============================================================================
# Downloads all source files referenced in Vorpal.lock, organized by platform.
#
# Usage: ./script/source.sh
#
# Downloads to: ${PWD}/source/<platform>/<filename>
# =============================================================================

LOCKFILE="${PWD}/Vorpal.lock"
SOURCE_DIR="${PWD}/source"

if [ ! -f "$LOCKFILE" ]; then
    echo "Error: Vorpal.lock not found at ${LOCKFILE}" >&2
    exit 1
fi

# Parse Vorpal.lock to extract (platform, url) pairs.
# Reads path and platform fields from each [[sources]] entry.
# Output: one "platform url" pair per line, deduplicated.
parse_sources() {
    awk '
        /^\[\[sources\]\]/ { url=""; plat="" }
        /^path = "/ { url=$0; sub(/^path = "/, "", url); sub(/"$/, "", url) }
        /^platform = "/ { plat=$0; sub(/^platform = "/, "", plat); sub(/"$/, "", plat) }
        length(url) > 0 && length(plat) > 0 { print plat " " url; url=""; plat="" }
    ' "$LOCKFILE" | sort -u
}

pairs=$(parse_sources)

if [ -z "$pairs" ]; then
    echo "No sources found in Vorpal.lock"
    exit 0
fi

total=$(echo "$pairs" | wc -l | tr -d ' ')
count=0
failed_urls=()

echo "Found ${total} unique source(s) to download"

while IFS=' ' read -r platform url; do
    count=$((count + 1))
    filename=$(basename "${url%%\?*}")
    target_dir="${SOURCE_DIR}/${platform}"
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
        continue
    fi
done <<< "$pairs"

if [ "${#failed_urls[@]}" -gt 0 ]; then
    echo "" >&2
    echo "Failed to download ${#failed_urls[@]} source(s):" >&2
    for entry in "${failed_urls[@]}"; do
        platform="${entry%% *}"
        url="${entry#* }"
        echo "  - [${platform}] ${url}" >&2
    done
    exit 1
fi

echo "Done"
