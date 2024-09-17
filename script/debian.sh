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
    flex \
    gawk \
    gettext \
    help2man \
    m4 \
    perl \
    texinfo \
    unzip
