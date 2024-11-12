#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update
sudo apt-get install \
    --no-install-recommends \
    --yes \
    ca-certificates \
    curl

if [ ! -d /etc/apt/keyrings ]; then
    sudo install -m 0755 -d /etc/apt/keyrings
fi

if [ ! -f /etc/apt/keyrings/docker.asc ]; then
    sudo curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
    sudo chmod a+r /etc/apt/keyrings/docker.asc
fi

if [ ! -f /etc/apt/sources.list.d/docker.list ]; then
    echo \
      "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/debian \
      $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
      sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
fi

sudo apt-get update
sudo apt-get install \
    --no-install-recommends \
    --yes \
    bubblewrap \
    containerd.io \
    docker-buildx-plugin \
    docker-ce \
    docker-ce-cli \
    docker-compose-plugin \
    unzip

sudo usermod -aG docker "${USER}"
