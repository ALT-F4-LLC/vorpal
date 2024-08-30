#!/usr/bin/env bash
set -e

ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
WORKDIR=$(pwd)

export NICKEL_IMPORT_PATH="$WORKDIR/.vorpal/packages:$WORKDIR"
export OPENSSL_DIR="$WORKDIR/deps/openssl"
export PATH="$WORKDIR/deps/proto/bin:$WORKDIR/deps/just:$HOME/.cargo/bin:$PATH"

just() {
    JUST_SYSTEM=""
    JUST_VERSION="1.35.0"

    if [[ "${OS}" == "darwin" ]]; then
        JUST_SYSTEM="apple-darwin"
    elif [[ "${OS}" == "linux" ]]; then
        JUST_SYSTEM="unknown-linux-gnu"
    else
        echo "Unsupported OS: ${OS}"
        exit 1
    fi

    if [[ "${ARCH}" == "x86_64" ]]; then
        JUST_SYSTEM="x86_64-${JUST_SYSTEM}"
    elif [[ "${ARCH}" == "arm64" ]]; then
        JUST_SYSTEM="aarch64-${JUST_SYSTEM}"
    else
        echo "Unsupported ARCH: ${ARCH}"
        exit 1
    fi

    JUST_URL="https://github.com/casey/just/releases/download/${JUST_VERSION}/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz"

    mkdir -p deps/just

    curl -L "${JUST_URL}" -o deps/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz

    tar -xvf deps/just-${JUST_VERSION}-${JUST_SYSTEM}.tar.gz -C deps/just
}

openssl() {
    OPENSSL_VERSION="3.3.1"
    OPENSSL_URL="https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz"

    curl -L "${OPENSSL_URL}" -o deps/openssl-${OPENSSL_VERSION}.tar.gz

    tar -xvf deps/openssl-${OPENSSL_VERSION}.tar.gz -C deps

    cd deps/openssl-${OPENSSL_VERSION}

    ./Config --prefix="${WORKDIR}/deps/openssl"

    make -j"$(nproc)"

    make install
}

protoc() {
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
    elif [[ "${ARCH}" == "aarch64" ]]; then
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

    curl -L "${PROTOC_URL}" -o deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip

    rm -rf deps/proto

    mkdir -p deps/proto

    unzip deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip -d deps/proto
}

mkdir -p ./deps

if [[ ! -d "./deps/just" ]]; then
    just
fi

if [[ ! -d "./deps/openssl" ]]; then
    openssl
fi

if [[ ! -d "./deps/proto" ]]; then
    protoc
fi

if ! command -v cargo &> /dev/null || [[ ! -x "$(command -v cargo)" ]]; then
    echo "cargo is not installed or not executable"
    exit 1
fi

if ! command -v rustc &> /dev/null || [[ ! -x "$(command -v rustc)" ]]; then
    echo "rustc is not installed or not executable"
    exit 1
fi

cargo install nickel-lang-cli

"$@"
