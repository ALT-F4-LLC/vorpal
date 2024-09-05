#!/usr/bin/env bash
set -euo pipefail

# ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

export SCRIPT_PATH="${PWD}/script"
export SCRIPT_PATH_INSTALL="${SCRIPT_PATH}/install"
export VORPAL_PATH="/var/lib/vorpal"
export VORPAL_PATH_SANDBOX="${VORPAL_PATH}/sandbox"
export VORPAL_PATH_STORE="${VORPAL_PATH}/store"

sudo mkdir -p "${VORPAL_PATH_SANDBOX}"
sudo mkdir -p "${VORPAL_PATH_STORE}"
sudo chown -R "$(id -u):$(id -g)" "${VORPAL_PATH}"

if [[ "${OS}" == "linux" ]]; then
    "${SCRIPT_PATH_INSTALL}/binutils.sh"
    "${SCRIPT_PATH_INSTALL}/gcc.sh"
fi

"${SCRIPT_PATH_INSTALL}/bash.sh"
"${SCRIPT_PATH_INSTALL}/coreutils.sh"
