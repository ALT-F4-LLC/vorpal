#!/usr/bin/env bash
set -euxo pipefail

ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ROOT_PATH=$(pwd)
RUSTUP_CONFIRM=false
SANDBOX_PATH="${ROOT_PATH}/sandbox"

for arg in "$@"; do
    case $arg in
        --rustup)
        RUSTUP_CONFIRM=true
        shift
        ;;
    esac
done

mkdir -p "${SANDBOX_PATH}"

if [[ "${OS}" == "linux" ]]; then
    "${ROOT_PATH}/script/install/apt.sh"

    if [[ ! -f "$SANDBOX_PATH/bin/ld" ]]; then
        "${ROOT_PATH}/script/install/binutils.sh" "${SANDBOX_PATH}"
    fi

    if [[ ! -f "$SANDBOX_PATH/bin/gcc" ]]; then
        "${ROOT_PATH}/script/install/gcc.sh" "${SANDBOX_PATH}"
    fi
fi

if [[ ! -f "$SANDBOX_PATH/bin/bash" ]]; then
    "${ROOT_PATH}/script/install/bash.sh" "${SANDBOX_PATH}"
fi

if [[ ! -f "$SANDBOX_PATH/bin/cat" ]]; then
    "${ROOT_PATH}/script/install/coreutils.sh" "${SANDBOX_PATH}"
fi

if ! command -v rustup &> /dev/null || [[ ! -x "$(command -v rustup)" ]]; then
    "${ROOT_PATH}/script/install/rustup.sh" "${RUSTUP_CONFIRM}"
fi

rustup show active-toolchain

if ! command -v cargo &> /dev/null || [[ ! -x "$(command -v cargo)" ]]; then
    echo "cargo is not installed or not executable"
    exit 1
fi

if ! command -v rustc &> /dev/null || [[ ! -x "$(command -v rustc)" ]]; then
    echo "rustc is not installed or not executable"
    exit 1
fi

if [[ ! -f "${SANDBOX_PATH}/bin/nickel" ]]; then
    "${ROOT_PATH}/script/install/nickel.sh" "${SANDBOX_PATH}" "${ARCH}" "${OS}"
fi

if [[ ! -f "${SANDBOX_PATH}/bin/protoc" ]]; then
    "${ROOT_PATH}/script/install/protoc.sh" "${SANDBOX_PATH}" "${ARCH}" "${OS}"
fi
