#!/bin/sh
set -euo pipefail

# Setup directories
sudo mkdir -p /var/lib/vorpal
sudo chown "$(id -u):$(id -g)" /var/lib/vorpal

# Unpack the binary
# tar -xzf ./dist/vorpal-aarch64-linux.tar.gz -C ./dist

# Setup keys
# ./dist/vorpal keys generate
