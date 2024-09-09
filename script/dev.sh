#!/usr/bin/env bash
set -euo pipefail

export ENV_PATH="${PWD}/.env"
readonly SCRIPT_PATH="${PWD}/script"

scripts=(
  "rustup.sh"
  "amber.sh"
  "nickel.sh" # must go after rustup.sh
  "protoc.sh"
  "zstd.sh"
)

mkdir -p "${ENV_PATH}/bin"

for script in "${scripts[@]}";
do
  "${SCRIPT_PATH}/dev/${script}" "${ENV_PATH}"
done

export PATH="${ENV_PATH}/bin:${HOME}/.cargo/bin:$PATH"

"$@"
