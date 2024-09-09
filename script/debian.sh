#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 [dev|sandbox]"
    exit 1
fi

sudo apt-get update

case "$1" in
    dev)
        sudo apt-get install \
            --no-install-recommends \
            --yes \
            bubblewrap \
            coreutils \
            direnv \
            unzip
        echo "eval \"\$(direnv hook bash)\"" >> ~/.bashrc
        ;;
    sandbox)
        sudo apt-get install \
            --no-install-recommends \
            --yes \
            autoconf \
            automake \
            bison \
            build-essential \
            coreutils \
            flex \
            gawk \
            gperf \
            m4 \
            perl \
            texinfo \
            zstd
        ;;
    *)
        usage
        ;;
esac
