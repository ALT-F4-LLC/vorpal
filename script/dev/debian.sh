#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install --yes \
    bubblewrap \
    ca-certificates \
    curl \
    unzip

if ! command -v docker &> /dev/null; then
    echo "Docker not found. Installing Docker..."

    curl -fsSL https://get.docker.com -o ./get-docker.sh

    sudo sh ./get-docker.sh

    rm ./get-docker.sh

    sudo usermod -aG docker "${USER}"
fi
