#!/usr/bin/env bash
set -euxo pipefail

ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
WORKDIR=$(pwd)

export NICKEL_IMPORT_PATH="$WORKDIR/.vorpal/packages:$WORKDIR"
export OPENSSL_DIR="$WORKDIR/deps/openssl"
export PATH="$WORKDIR/deps/protoc/bin:$WORKDIR/deps/just:$HOME/.cargo/bin:$PATH"

cd "${WORKDIR}"

mkdir -p ./deps

if [[ ! -d "$PWD/deps/just" ]]; then
    JUST_SYSTEM=""
    JUST_VERSION="1.35.0"

    if [[ "${OS}" == "darwin" ]]; then
        JUST_SYSTEM="apple-darwin"
    elif [[ "${OS}" == "linux" ]]; then
        JUST_SYSTEM="unknown-linux-musl"
    else
        echo "Unsupported OS: ${OS}"
        exit 1
    fi

    if [[ "${ARCH}" == "x86_64" ]]; then
        JUST_SYSTEM="x86_64-${JUST_SYSTEM}"
    elif [[ "${ARCH}" == "arm64" || "${ARCH}" == "aarch64" ]]; then
        JUST_SYSTEM="aarch64-${JUST_SYSTEM}"
    else
        echo "Unsupported ARCH: ${ARCH}"
        exit 1
    fi

    JUST_URL="https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz"

    mkdir -p deps/just

    curl -L "${JUST_URL}" -o deps/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz

    tar -xvf deps/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz -C deps/just

    rm -rf deps/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz

    # TODO: support hash checking for downloads
fi

cd "${WORKDIR}"

if [[ ! -d "$PWD/deps/openssl" ]]; then
    OPENSSL_VERSION="3.3.1"
    OPENSSL_URL="https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz"

    curl -L "${OPENSSL_URL}" -o deps/openssl-${OPENSSL_VERSION}.tar.gz

    tar -xvf deps/openssl-${OPENSSL_VERSION}.tar.gz -C deps

    cd deps/openssl-${OPENSSL_VERSION}

    ./Configure --prefix="${WORKDIR}/deps/openssl"

    make -j$(sysctl -n hw.ncpu)

    make install

    rm -rf "${WORKDIR}/deps/openssl-${OPENSSL_VERSION}"

    rm -rf "${WORKDIR}/deps/openssl-${OPENSSL_VERSION}.tar.gz"

    # TODO: support hash checking for downloads
fi

cd "${WORKDIR}"

if [[ ! -d "$PWD/deps/protoc" ]]; then
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

if ! command -v cargo &> /dev/null || [[ ! -x "$(command -v cargo)" ]]; then
    echo "cargo is not installed or not executable"
    exit 1
fi

if ! command -v rustc &> /dev/null || [[ ! -x "$(command -v rustc)" ]]; then
    echo "rustc is not installed or not executable"
    exit 1
fi

# cargo install nickel-lang-cli

"$@"
