#!/usr/bin/env bash
set -euo pipefail

sudo apt-get update

sudo apt-get install \
    --no-install-recommends \
    --yes \
    bubblewrap \
    unzip
