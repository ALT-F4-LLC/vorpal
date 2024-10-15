#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install \
    --no-install-recommends \
    --yes \
    autoconf \
    automake \
    bash \
    binutils \
    bison \
    bubblewrap \
    build-essential \
    bzip2 \
    ca-certificates \
    coreutils \
    diffutils \
    file \
    findutils \
    flex \
    g++ \
    gawk \
    gcc \
    gettext \
    grep \
    gzip \
    help2man \
    libbison-dev \
    libc6-dev \
    m4 \
    make \
    patch \
    patchelf \
    perl \
    pkg-config \
    python3 \
    ripgrep \
    rsync \
    sed \
    tar \
    texinfo \
    unzip \
    xz-utils \
    zlib1g-dev
