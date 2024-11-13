#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install --yes \
    bubblewrap \
    ca-certificates \
    curl \
    unzip

curl -fsSL https://get.docker.com -o ./get-docker.sh

sudo sh ./get-docker.sh

rm ./get-docker.sh

sudo usermod -aG docker "${USER}"
