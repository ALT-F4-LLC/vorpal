#!/usr/bin/env bash
set -euo pipefail

ARCH=$(uname -m | tr '[:upper:]' '[:lower:]')
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ROOT_PATH=$(pwd)
RUSTUP_CONFIRM=false
SANDBOX_PATH="${ROOT_PATH}/sandbox"

# Parse arguments

for arg in "$@"; do
    case $arg in
        --rustup)
        RUSTUP_CONFIRM=true
        shift
        ;;
    esac
done

# Create sandbox directory

mkdir -p "${SANDBOX_PATH}"

# Verify 'linux' dependencies

if [[ "${OS}" == "linux" ]]; then
    sudo apt-get update

    sudo apt-get install build-essential --no-install-recommends --yes

    # Verify 'm4' installed

    if [[ ! -f "$SANDBOX_PATH/bin/m4" ]]; then
        echo "---> Installing M4..."

        M4_VERSION="1.4.19"

        curl -L \
            "https://ftp.gnu.org/gnu/m4/m4-${M4_VERSION}.tar.gz" \
            -o "/tmp/m4-${M4_VERSION}.tar.gz"
        tar -xzf "/tmp/m4-${M4_VERSION}.tar.gz" -C "/tmp"

        pushd "/tmp/m4-${M4_VERSION}"

        ./configure \
            --disable-shared \
            --enable-static \
            --prefix="${SANDBOX_PATH}"
        make
        make install

        popd

        rm -rf "/tmp/m4-${M4_VERSION}"
        rm -f "/tmp/m4-${M4_VERSION}.tar.gz"

        echo "---> M4 installed"
    fi

    # Verify 'gmp' installed

    if [[ ! -f "$SANDBOX_PATH/include/gmp.h" ]]; then
        echo "---> Installing GMP..."

        GMP_VERSION="6.3.0"

        curl -L \
            "https://gmplib.org/download/gmp/gmp-${GMP_VERSION}.tar.gz" \
            -o "/tmp/gmp-${GMP_VERSION}.tar.gz"
        tar -xzf "/tmp/gmp-${GMP_VERSION}.tar.gz" -C "/tmp"

        pushd "/tmp/gmp-${GMP_VERSION}"

        export M4="${SANDBOX_PATH}/bin/m4"

        ./configure \
            --build="aarch64-unknown-linux-gnu" \
            --disable-shared \
            --enable-static \
            --prefix="${SANDBOX_PATH}"
        make
        make check
        make install

        popd

        rm -rf "/tmp/gmp-${GMP_VERSION}"
        rm -f "/tmp/gmp-${GMP_VERSION}.tar"
        rm -f "/tmp/gmp-${GMP_VERSION}.tar.gz"

        echo "---> GMP installed"
    fi

    # Verify 'mpfr' installed

    if [[ ! -f "$SANDBOX_PATH/include/mpfr.h" ]]; then
        echo "---> Installing MPFR..."

        MPFR_VERSION="4.2.1"

        curl -L \
            "https://www.mpfr.org/mpfr-current/mpfr-${MPFR_VERSION}.tar.gz" \
            -o "/tmp/mpfr-${MPFR_VERSION}.tar.gz"
        tar -xzf "/tmp/mpfr-${MPFR_VERSION}.tar.gz" -C "/tmp"

        pushd "/tmp/mpfr-${MPFR_VERSION}"

        ./configure \
            --disable-shared \
            --enable-static \
            --with-gmp="${SANDBOX_PATH}" \
            --prefix="${SANDBOX_PATH}"
        make
        make check
        make install

        popd

        rm -rf "/tmp/mpfr-${MPFR_VERSION}"
        rm -f "/tmp/mpfr-${MPFR_VERSION}.tar.gz"

        echo "---> MPFR installed"
    fi

    # Verify 'mpc' installed

    if [[ ! -f "$SANDBOX_PATH/include/mpc.h" ]]; then
        echo "---> Installing MPC..."

        MPC_VERSION="1.3.1"

        curl -L \
            "https://ftp.gnu.org/gnu/mpc/mpc-${MPC_VERSION}.tar.gz" \
            -o "/tmp/mpc-${MPC_VERSION}.tar.gz"
        tar -xzf "/tmp/mpc-${MPC_VERSION}.tar.gz" -C "/tmp"

        pushd "/tmp/mpc-${MPC_VERSION}"

        ./configure \
            --disable-shared \
            --enable-static \
            --with-gmp="${SANDBOX_PATH}" \
            --with-mpfr="${SANDBOX_PATH}" \
            --prefix="${SANDBOX_PATH}"
        make
        make check
        make install

        popd

        rm -rf "/tmp/mpc-${MPC_VERSION}"
        rm -f "/tmp/mpc-${MPC_VERSION}.tar.gz"

        echo "---> MPC installed"
    fi

    # Verify 'gcc' installed and executable

    if [[ ! -f "$SANDBOX_PATH/bin/gcc" ]]; then
        echo "---> Installing GCC..."

        GCC_VERSION="14.2.0"

        curl -L \
            "https://ftp.gnu.org/gnu/gcc/gcc-${GCC_VERSION}/gcc-${GCC_VERSION}.tar.gz" \
            -o "/tmp/gcc-${GCC_VERSION}.tar.gz"

        tar -xzf "/tmp/gcc-${GCC_VERSION}.tar.gz" -C "/tmp"

        mkdir -p /tmp/gcc-${GCC_VERSION}/build

        pushd "/tmp/gcc-${GCC_VERSION}/build"

        ../configure \
            --disable-shared \
            --enable-static \
            --prefix="${SANDBOX_PATH}" \
            --with-gmp="${SANDBOX_PATH}" \
            --with-mpc="${SANDBOX_PATH}" \
            --with-mpfr="${SANDBOX_PATH}"
        make
        make install

        popd

        rm -rf "/tmp/gcc-${GCC_VERSION}/build"
        rm -rf "/tmp/gcc-${GCC_VERSION}"
        rm -f "/tmp/gcc-${GCC_VERSION}.tar.gz"

        echo "---> GCC installed"
    fi
fi

# Verify 'bash' installed

if [[ ! -f "$SANDBOX_PATH/bin/bash" ]]; then
    echo "---> Installing Bash..."

    BASH_VERSION="5.2"

    curl -L \
        "https://ftp.gnu.org/gnu/bash/bash-${BASH_VERSION}.tar.gz" \
        -o "/tmp/bash-${BASH_VERSION}.tar.gz"
    tar -xzf "/tmp/bash-${BASH_VERSION}.tar.gz" -C "/tmp"

    pushd "/tmp/bash-${BASH_VERSION}"

    ./configure --prefix="${SANDBOX_PATH}"
    make
    make install

    popd

    rm -rf "/tmp/bash-${BASH_VERSION}"
    rm -f "/tmp/bash-${BASH_VERSION}.tar.gz"

    echo "---> Bash installed"
fi

# Verify 'coreutils' installed

if [[ ! -f "$SANDBOX_PATH/bin/cat" ]]; then
    echo "---> Installing Coreutils..."

    COREUTILS_VERSION="9.5"

    curl -L \
        "https://ftp.gnu.org/gnu/coreutils/coreutils-${COREUTILS_VERSION}.tar.gz" \
        -o "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"
    tar -xzf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz" -C "/tmp"

    pushd "/tmp/coreutils-${COREUTILS_VERSION}"

    ./configure --prefix="${SANDBOX_PATH}"
    make
    make install

    popd

    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}"
    rm -rf "/tmp/coreutils-${COREUTILS_VERSION}.tar.gz"

    echo "---> Coreutils installed"
fi

# Verify 'rustup' installed and executable

if ! command -v rustup &> /dev/null || [[ ! -x "$(command -v rustup)" ]]; then
    if $RUSTUP_CONFIRM; then
        confirm="y"
    else
        read -r -p "Do you want to install rustup? (y/n): " confirm
    fi

    if [[ "$confirm" == "y" ]]; then
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
            | sh -s -- --default-toolchain 'none' --no-modify-path --profile 'minimal' -y
    else
        echo "Installation aborted."
        exit 1
    fi
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

# Verify 'nickel' installed

if [[ ! -f "${SANDBOX_PATH}/bin/nickel" ]]; then
    echo "---> Installing Nickel..."

    NICKEL_ARCH=$ARCH
    NICKEL_VERSION="1.7.0"

    if [ "$ARCH" = "aarch64" ]; then
        NICKEL_ARCH="arm64";
    fi

    if [ "$OS" == "darwin" ]; then
        cargo install nickel-lang-cli

        cp "$(which nickel)" "${SANDBOX_PATH}/bin/nickel"
    fi

    if [ "$OS" == "linux" ]; then
        curl -L \
            "https://github.com/tweag/nickel/releases/download/${NICKEL_VERSION}/nickel-${NICKEL_ARCH}-linux" \
            -o "${SANDBOX_PATH}/bin/nickel"

        chmod +x "${SANDBOX_PATH}/bin/nickel"
    fi

    echo "---> Nickel installed"
fi

if [[ ! -f "${SANDBOX_PATH}/bin/protoc" ]]; then
    echo "---> Installing Protobuf..."

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

    echo "---> Protobuf installed"
fi

# export NICKEL_IMPORT_PATH="$ROOT_PATH/.vorpal/packages:$ROOT_PATH"
# export PATH="${ROOT_PATH}/sandbox/bin:${HOME}/.cargo/bin:$PATH"

"$@"
