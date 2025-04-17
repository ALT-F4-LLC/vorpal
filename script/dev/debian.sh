#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install --yes \
    bubblewrap \
    build-essential \
    ca-certificates \
    curl \
    jq \
    rsync \
    unzip

if ! command -v docker &> /dev/null; then
    echo "Docker not found. Installing Docker..."

    curl -fsSL https://get.docker.com -o /tmp/get-docker.sh

    sudo sh /tmp/get-docker.sh

    rm /tmp/get-docker.sh

    sudo usermod -aG docker "${USER}"
fi
