#!/bin/bash
set -euo pipefail

# Setup directories
sudo mkdir -p /var/lib/vorpal/{cache,key,sandbox,store}
sudo chown -R "$(id -u):$(id -g)" /var/lib/vorpal
