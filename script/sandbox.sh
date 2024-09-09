#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m | tr '[:upper:]' '[:lower:]')"
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
VORPAL_PATH="/var/lib/vorpal"

if [[ "${ARCH}" == "arm64" ]]; then
    ARCH="aarch64"
fi

SANDBOX_HASH=$(cat "${PWD}/script/sandbox/sha256sum/${ARCH}-${OS}/sandbox")
SANDBOX_STORE_PATH="${VORPAL_PATH}/store/vorpal-sandbox-${SANDBOX_HASH}"
SANDBOX_STORE_PATH_PACKAGE="${SANDBOX_STORE_PATH}.package"

if [[ -d "${SANDBOX_STORE_PATH_PACKAGE}" ]]; then
    echo "sandbox exists: ${SANDBOX_STORE_PATH_PACKAGE}"
    exit 1
fi

directories=(
    "${VORPAL_PATH}"
    "${VORPAL_PATH}/sandbox"
    "${VORPAL_PATH}/store"
)

packages_darwin=(
    "binutils"
    "bash"
    "coreutils"
    "zstd"
)
packages_hashes=()
packages_installed=()
packages_linux=(
    "binutils"
    "gcc"
    "linux-headers"
    "glibc"
    "bash"
    "coreutils"
    "zstd"
    # "libstdc++"
)

for dir in "${directories[@]}"; do
    if [[ ! -d "${dir}" ]]; then
        sudo mkdir -p "${dir}"
        sudo chown -R "$(id -u):$(id -g)" "${dir}"
    fi
done

mkdir -p "${SANDBOX_STORE_PATH_PACKAGE}"

if [[ "${OS}" == "darwin" ]]; then
    for package in "${packages_darwin[@]}"; do
        "${PWD}/script/sandbox/${package}.sh" "${SANDBOX_STORE_PATH_PACKAGE}"
        hash="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/${package}")"
        packages_hashes+=("${hash}")
        packages_installed+=("${package}")
    done
fi

if [[ "${OS}" == "linux" ]]; then
    for package in "${packages_linux[@]}"; do
        "${PWD}/script/sandbox/${package}.sh" "${SANDBOX_STORE_PATH_PACKAGE}"
        hash="$(cat "${PWD}/script/sandbox/sha256sum/${OS}/${package}")"
        packages_hashes+=("${hash}")
        packages_installed+=("${package}")
    done
fi

source_hash=$(echo "${packages_hashes[@]}" | shasum -a 256 | awk '{print $1}')

if [[ "${SANDBOX_HASH}" != "${source_hash}" ]]; then
    echo "sandbox hash mismatch: ${SANDBOX_HASH} != ${source_hash}"
    rm -rf "${SANDBOX_STORE_PATH_PACKAGE}"
    exit 1
fi

# Patch for includes
mkdir -p "${SANDBOX_STORE_PATH_PACKAGE}/usr/include"
rsync -av --ignore-existing "${SANDBOX_STORE_PATH_PACKAGE}/include/" "${SANDBOX_STORE_PATH_PACKAGE}/usr/include"
rm -rf "${SANDBOX_STORE_PATH_PACKAGE}/include"

# Patch for linux only
if [[ "${OS}" == "linux" ]]; then
    # Patch for glibc
    ln -s "${SANDBOX_STORE_PATH_PACKAGE}/bin/gcc" "${SANDBOX_STORE_PATH_PACKAGE}/bin/cc"

    # Patch for nameservers
    echo "nameserver 1.1.1.1" > "${SANDBOX_STORE_PATH_PACKAGE}/etc/resolv.conf"

    # Copy /etc/ssl/certs to sandbox
    mkdir -p "${SANDBOX_STORE_PATH_PACKAGE}/etc/ssl/certs"
    rsync -av /etc/ssl/certs/ "${SANDBOX_STORE_PATH_PACKAGE}/etc/ssl/certs"
fi

# Compress sandbox
tar -cvf - -C "${SANDBOX_STORE_PATH_PACKAGE}" . | zstd -o "${SANDBOX_STORE_PATH_PACKAGE}.tar.zst"
