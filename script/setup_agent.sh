#!/usr/bin/env bash
set -euxo pipefail

NIX_CONFIG_BINARIES="environment\.systemPackages = \[pkgs\.automake pkgs\.autoconf pkgs\.bubblewrap pkgs\.gcc pkgs\.git\ pkgs\.gnumake pkgs\.help2man pkgs\.patchelf pkgs\.perl pkgs\.vim];"
NIX_CONFIG_FEATURES="nix\.settings\.experimental-features = \[\"nix-command\" \"flakes\"\];"
NIX_CONFIG_PATH="/etc/nixos/configuration.nix"

if ! grep -q "$NIX_CONFIG_FEATURES" "$NIX_CONFIG_PATH"; then
    echo "Adding features to $NIX_CONFIG_PATH"
    sed -i "s/^}/  $NIX_CONFIG_FEATURES\n}/" "$NIX_CONFIG_PATH"
fi

if ! grep -q "$NIX_CONFIG_BINARIES" "$NIX_CONFIG_PATH"; then
    echo "Adding binaries to $NIX_CONFIG_PATH"
    sed -i "s/^}/  $NIX_CONFIG_BINARIES\n}/" "$NIX_CONFIG_PATH"
fi

nixos-rebuild build
nixos-rebuild test
nixos-rebuild switch
