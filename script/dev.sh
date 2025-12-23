#!/usr/bin/env bash
set -euo pipefail

export ENV_PATH="${PWD}/.env"
readonly SCRIPT_PATH="${PWD}/script"

# TODO: add lima and qemu installation

scripts=("lima" "rustup" "protoc" "terraform")

if [[ "$(uname -s)" == "Linux" ]]; then
    . /etc/os-release
    if [[ "$ID" == "debian" || "$ID" == "ubuntu" ]]; then
        "${SCRIPT_PATH}/dev/debian.sh"
    elif [[ "$ID" == "arch" ]]; then
        "${SCRIPT_PATH}/dev/arch.sh"
    else
        echo "Unknown Linux distribution."
        echo "You will need to install the following manually:"
        echo ""
        echo "    bubblewrap ca-certificates curl unzip docker"
    fi
fi

mkdir -p "${ENV_PATH}/bin"

for script in "${scripts[@]}";
do
  "${SCRIPT_PATH}/dev/${script}.sh" "${ENV_PATH}"
done

export PATH="${ENV_PATH}/bin:${HOME}/.cargo/bin:$PATH"

"$@"
