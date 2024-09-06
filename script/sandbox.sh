#!/usr/bin/env bash
set -euo pipefail

# ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

directories=("/var/lib/vorpal/sandbox" "/var/lib/vorpal/store")

for dir in "${directories[@]}"; do
    sudo mkdir -p "${dir}"
done

sudo chown -R "$(id -u):$(id -g)" "/var/lib/vorpal"

linux_scripts=(
    "gcc.sh"
)

common_scripts=(
    "bash.sh"
    "binutils.sh"
    "coreutils.sh"
    "zstd.sh"
)

if [[ "${OS}" == "linux" ]]; then
    for script in "${linux_scripts[@]}"; do
        "${PWD}/script/sandbox/${script}"
    done
fi

for script in "${common_scripts[@]}"; do
    "${PWD}/script/sandbox/${script}"
done
