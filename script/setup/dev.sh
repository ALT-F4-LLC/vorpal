#!/usr/bin/env bash
set -euo pipefail

export VORPAL_PATH="${PWD}/.vorpal"
export VORPAL_PATH_ENV="${VORPAL_PATH}/env"
export VORPAL_PATH_ENV_BIN="${VORPAL_PATH_ENV}/bin"
readonly SCRIPT_PATH="${PWD}/script/install"

scripts=(
  "rustup.sh"
  "nickel.sh"
  "protoc.sh"
)

mkdir -p "${VORPAL_PATH_ENV_BIN}"

for script in "${scripts[@]}";
do
  "${SCRIPT_PATH}/${script}"
done
