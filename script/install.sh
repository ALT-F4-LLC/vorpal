#!/bin/bash
set -euo pipefail

# Setup root directory
sudo mkdir -p /var/lib/vorpal
sudo chown "$(id -u):$(id -g)" /var/lib/vorpal

# TODO: Generate keys
# vorpal keys generate
