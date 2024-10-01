#!/usr/bin/env bash
set -euo pipefail

export ENV_PATH="${PWD}/.env"
readonly SCRIPT_PATH="${PWD}/script"

scripts=(
  "rustup"
  "nickel" # must go after rustup.sh
  "protoc"
  "zstd"
)

mkdir -p "${ENV_PATH}/bin"

for script in "${scripts[@]}";
do
  "${SCRIPT_PATH}/dev/${script}.sh" "${ENV_PATH}"
done

export PATH="${ENV_PATH}/bin:${HOME}/.cargo/bin:$PATH"

"$@"
