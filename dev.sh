#!/usr/bin/env bash
set -euo pipefail

WORKDIR=$(pwd)
ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

export NICKEL_IMPORT_PATH="$WORKDIR/.vorpal/packages:$WORKDIR"
export PATH="$WORKDIR/deps/nickel/bin:$WORKDIR/deps/openssl/bin:$WORKDIR/deps/protoc/bin:$HOME/.cargo/bin:$PATH"

rustup show active-toolchain

if ! command -v cargo &> /dev/null || [[ ! -x "$(command -v cargo)" ]]; then
    echo "cargo is not installed or not executable"
    exit 1
fi

if ! command -v rustc &> /dev/null || [[ ! -x "$(command -v rustc)" ]]; then
    echo "rustc is not installed or not executable"
    exit 1
fi

mkdir -p ./deps

cd "${WORKDIR}"

if [[ ! -d "$PWD/deps/nickel/bin" ]]; then
    NICKEL_ARCH=$ARCH

    if [ "$ARCH" = "aarch64" ]; then
        NICKEL_ARCH="arm64";
    fi

    mkdir -p deps/nickel/bin

    if [ "$OS" == "darwin" ]; then
        cargo install nickel-lang-cli

        cp "$(which nickel)" deps/nickel/bin/nickel
    fi

    if [ "$OS" == "linux" ]; then
        curl -fsSL \
            "https://github.com/tweag/nickel/releases/download/1.7.0/nickel-${NICKEL_ARCH}-linux" \
            -o deps/nickel/bin/nickel

        chmod +x deps/nickel/bin/nickel
    fi
fi

cd "${WORKDIR}"

if [[ ! -d "$PWD/deps/protoc/bin" ]]; then
    PROTOC_SYSTEM=""
    PROTOC_VERSION="28.0"

    if [[ "${OS}" == "darwin" ]]; then
        PROTOC_SYSTEM="osx"
    elif [[ "${OS}" == "linux" ]]; then
        PROTOC_SYSTEM="linux"
    else
        echo "Unsupported OS: ${OS}"
        exit 1
    fi

    if [[ "${ARCH}" == "x86_64" ]]; then
        PROTOC_SYSTEM="${PROTOC_SYSTEM}-x86_64"
    elif [[ "${ARCH}" == "arm64" || "${ARCH}" == "aarch64" ]]; then
        PROTOC_SYSTEM="${PROTOC_SYSTEM}-aarch_64"
    else
        echo "Unsupported ARCH: ${ARCH}"
        exit 1
    fi

    if [[ "$PROTOC_SYSTEM" == "" ]]; then
        echo "PROTOC_SYSTEM is empty"
        exit 1
    fi

    PROTOC_URL="https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"

    mkdir -p deps/protoc

    curl -L "${PROTOC_URL}" -o deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip

    unzip deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip -d deps/protoc

    rm -rf deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip

    # TODO: support hash checking for downloads
fi

"$@"
