#!/usr/bin/env bash
set -euo pipefail

export ENV_PATH="${PWD}/.env"
readonly SCRIPT_PATH="${PWD}/script/install"

scripts=(
  "rustup.sh"
  "nickel.sh"
  "protoc.sh"
)

mkdir -p "${ENV_PATH}/bin"

for script in "${scripts[@]}";
do
  "${SCRIPT_PATH}/${script}"
done
