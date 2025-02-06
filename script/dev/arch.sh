#!/usr/bin/env bash
set -euo pipefail

sudo pacman -S --noconfirm docker bubblewrap ca-certificates curl unzip
sudo usermod -aG docker $USER
