#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install \
    --no-install-recommends \
    --yes \
    autoconf \
    automake \
    bison \
    bubblewrap \
    build-essential \
    ca-certificates \
    flex \
    gawk \
    gettext \
    help2man \
    m4 \
    make \
    patchelf \
    perl \
    rsync \
    texinfo \
    unzip \
    zlib1g-dev
