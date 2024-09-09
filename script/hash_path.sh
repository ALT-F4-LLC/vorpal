#!/bin/bash
set -euo pipefail

SHA_COMMAND="sha256sum"

if ! command -v "$SHA_COMMAND" &> /dev/null; then
  echo "Command not found: $SHA_COMMAND"
  exit 1
fi

if [ -z "$1" ]; then
  echo "Usage: $0 <path>"
  exit 1
fi

OUTPUT_HASH=()

generate_hash() {
  local file=$1
  local hash
  hash=$($SHA_COMMAND --binary --zero "$file" | awk '{print $1}')
  OUTPUT_HASH+=("$hash")
}

if [ -f "$1" ]; then
  generate_hash "$1"
elif [ -d "$1" ]; then
  while IFS= read -r -d '' file; do
    generate_hash "$file"
  done < <(find "$1" -type f -print0)
else
  echo "Invalid path: $1"
  exit 1
fi

if [ ${#OUTPUT_HASH[@]} -gt 1 ]; then
  COMBINED_HASH=$(printf "%s" "${OUTPUT_HASH[@]}" | $SHA_COMMAND | awk '{print $1}')
  echo "${COMBINED_HASH}"
else
  echo "${OUTPUT_HASH[0]}"
fi
