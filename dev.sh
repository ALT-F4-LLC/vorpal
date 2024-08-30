#!/usr/bin/env bash
set -e

OPENSSL_URL="https://github.com/openssl/openssl/releases/download/openssl-${OPENSSL_VERSION}/openssl-${OPENSSL_VERSION}.tar.gz"
OPENSSL_VERSION="3.3.1"
PROTOC_URL="https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-osx-aarch_64.zip"
PROTOC_VERSION="28.0"
WORKDIR=$(pwd)

openssl() {
    wget "${OPENSSL_URL}" -O deps/openssl-${OPENSSL_VERSION}.tar.gz

    tar -xvf deps/openssl-${OPENSSL_VERSION}.tar.gz -C deps

    cd deps/openssl-${OPENSSL_VERSION}

    mkdir -p "${WORKDIR}/deps/openssl-${OPENSSL_VERSION}-dist"

    ./Config --prefix="${WORKDIR}/deps/openssl-${OPENSSL_VERSION}-dist"

    make -j"$(nproc)"

    make install
}

protoc() {
    ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    PROTOC_SYSTEM=""

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

    wget "${PROTOC_URL}" -O deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip

    mkdir -p deps/proto-${PROTOC_VERSION}-${PROTOC_SYSTEM}

    unzip \
        deps/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip \
        -d deps/proto-${PROTOC_VERSION}-${PROTOC_SYSTEM}
}

mkdir -p ./deps

if [[ ! -d "./deps/openssl-${OPENSSL_VERSION}" ]]; then
    openssl
fi

if [[ ! -d "./deps/proto-${PROTOC_VERSION}-osx_aarch_64" ]]; then
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
