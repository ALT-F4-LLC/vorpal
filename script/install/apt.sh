#!/usr/bin/env bash
set -euo pipefail

echo "Install apt -> (build-essential)"

sudo apt-get update

sudo apt-get install \
    --no-install-recommends \
    --yes \
    build-essential
