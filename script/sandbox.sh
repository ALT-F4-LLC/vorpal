#!/usr/bin/env bash
set -euo pipefail

# ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VORPAL_PATH="/var/lib/vorpal"
SANDBOX_HASH=$(cat "${PWD}/script/sandbox.sha256sum")
SANDBOX_STORE_PATH="${VORPAL_PATH}/store/vorpal-sandbox-${SANDBOX_HASH}"
SANDBOX_STORE_PATH_PACKAGE="${SANDBOX_STORE_PATH}.package"

directories=(
    "${VORPAL_PATH}"
    "${VORPAL_PATH}/sandbox"
    "${VORPAL_PATH}/store"
)

linux_packages=(
    "gcc"
)

common_packages=(
    "bash"
    "binutils"
    "coreutils"
    "zstd"
)

packages_hashes=()

packages_installed=()

for dir in "${directories[@]}"; do
    if [[ ! -d "${dir}" ]]; then
        sudo mkdir -p "${dir}"
        sudo chown -R "$(id -u):$(id -g)" "${dir}"
    fi
done

if [[ "${OS}" == "linux" ]]; then
    for package in "${linux_packages[@]}"; do
        "${PWD}/script/sandbox/${package}.sh"
        hash="$(cat "${PWD}/script/sandbox/${package}.sha256sum")"
        packages_hashes+=("${hash}")
        packages_installed+=("${package}")
    done
fi

for package in "${common_packages[@]}"; do
    "${PWD}/script/sandbox/${package}.sh"
    hash="$(cat "${PWD}/script/sandbox/${package}.sha256sum")"
    packages_hashes+=("${hash}")
    packages_installed+=("${package}")
done

source_hash=$(echo "${packages_hashes[@]}" | shasum -a 256 | awk '{print $1}')

if [[ "${SANDBOX_HASH}" != "${source_hash}" ]]; then
    echo "source hash mismatch: ${SANDBOX_HASH} != ${source_hash}"
    exit 1
fi

if [[ -d "${SANDBOX_STORE_PATH_PACKAGE}" ]]; then
    echo "vorpal-sandbox-${SANDBOX_HASH}"
    exit 1
fi

for package in "${packages_installed[@]}"; do
    PACKAGE_HASH="$(cat "${PWD}/script/sandbox/${package}.sha256sum")"
    PACKAGE_PATH="${VORPAL_PATH}/store/${package}-${PACKAGE_HASH}.package"

    find "${PACKAGE_PATH}" -type f ! -path "${PACKAGE_PATH}/share/*" | while read -r file; do
        relative_path="${file#"${PACKAGE_PATH}/"}"

        mkdir -p "${SANDBOX_STORE_PATH_PACKAGE}/$(dirname "${relative_path}")"

        ln -s "${file}" "${SANDBOX_STORE_PATH_PACKAGE}/${relative_path}"
    done
done

tar -cvf - -C "${SANDBOX_STORE_PATH_PACKAGE}" . | zstd -o "${SANDBOX_STORE_PATH_PACKAGE}.tar.zst"
