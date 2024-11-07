#!/bin/bash
set -euo pipefail

# Setup directories
sudo mkdir -p /var/lib/vorpal
sudo chown "$(id -u):$(id -g)" /var/lib/vorpal

# Setup keys
./dist/vorpal keys generate
