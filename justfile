_default:
    just --list

_start profile:
    nix run '.#{{ profile }}'

build profile="default":
    nix build \
        --json \
        --no-link \
        --print-build-logs \
        '.#{{ profile }}'

check:
    nix flake check

start:
    just _start "vorpal-build"

start-build:
    just _start "vorpal-build"

package profile="default":
    #!/usr/bin/env bash
    set -euxo pipefail
    DERIVATION=$(just _build "{{ profile }}")
    OUTPUT=$(echo $DERIVATION | jq -r .[0].outputs.out)
    install --mode 755 $OUTPUT/bin/vorpal-build .
    install --mode 755 $OUTPUT/bin/vorpal-cli .

update:
    nix flake update
