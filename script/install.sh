#!/bin/bash
set -euo pipefail

# Setup directories
sudo mkdir -p /vorpal
sudo chown "$(id -u):$(id -g)" /vorpal

# Setup keys
./dist/vorpal keys generate
