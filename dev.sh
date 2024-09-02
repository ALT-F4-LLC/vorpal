#!/usr/bin/env bash
set -euo pipefail

ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ROOT_PATH=$(pwd)
SANDBOX_PATH="${ROOT_PATH}/sandbox"

export NICKEL_IMPORT_PATH="$ROOT_PATH/.vorpal/packages:$ROOT_PATH"
export PATH="${ROOT_PATH}/sandbox/bin:$HOME/.cargo/bin:$PATH"

# Verify 'rustup' installed and executable

if ! command -v rustup &> /dev/null || [[ ! -x "$(command -v rustup)" ]]; then
    echo "rustup is not installed or not executable"
    exit 1
fi

rustup show active-toolchain

# Verify 'cargo' and 'rustc' installed and executable

if ! command -v cargo &> /dev/null || [[ ! -x "$(command -v cargo)" ]]; then
    echo "cargo is not installed or not executable"
    exit 1
fi

if ! command -v rustc &> /dev/null || [[ ! -x "$(command -v rustc)" ]]; then
    echo "rustc is not installed or not executable"
    exit 1
fi

# Create sandbox directory

mkdir -p "${SANDBOX_PATH}"

# Verify 'bash' installed

if [[ ! -f "$SANDBOX_PATH/bin/bash" ]]; then
    BASH_VERSION="5.2"

    curl -L \
        "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz" \
        -o "/tmp/bash-${BASH_VERSION}.tar.gz"
    tar -xzf "/tmp/bash-${BASH_VERSION}.tar.gz" -C "/tmp"

    pushd "/tmp/bash-${BASH_VERSION}"

    ./configure --prefix="${SANDBOX_PATH}"
    make -j"$(nproc)"
    make install

    popd

    rm -rf "/tmp/bash-${BASH_VERSION}"
    rm -f "/tmp/bash-${BASH_VERSION}.tar.gz"
fi

# Verify 'coreutils' installed

if [[ ! -f "$SANDBOX_PATH/bin/cat" ]]; then
    COREUTILS_VERSION="9.5"

    curl -L \
        "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
        -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
    tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

    pushd "/tmp/coreutils-${COREUTILS_VERSION}"

    ./configure --prefix="${SANDBOX_PATH}"
    make -j"$(nproc)"
    make install

    popd

    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}"
    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
fi

# Verify 'nickel' installed

if [[ ! -f "${SANDBOX_PATH}/bin/nickel" ]]; then
    NICKEL_ARCH=$ARCH
    NICKEL_VERSION="1.7.0"

    if [ "$ARCH" = "aarch64" ]; then
        NICKEL_ARCH="arm64";
    fi

    if [ "$OS" == "darwin" ]; then
        cargo install nickel-lang-cli

        cp "$(which nickel)" "${SANDBOX_PATH}/sandbox/bin/nickel"
    fi

    if [ "$OS" == "linux" ]; then
        curl -L \
            "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
            -o "${SANDBOX_PATH}/bin/nickel"

        chmod +x "${SANDBOX_PATH}/bin/nickel"
    fi
fi

if [[ ! -f "${SANDBOX_PATH}/bin/protoc" ]]; then
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

    curl -L \
        "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip" \
        -o "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"
    unzip "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip" -d "${SANDBOX_PATH}"

    rm -rf "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}"
    rm -f "/tmp/protoc-${PROTOC_VERSION}-${PROTOC_SYSTEM}.zip"

    # TODO: support hash checking for downloads
fi

"$@"
