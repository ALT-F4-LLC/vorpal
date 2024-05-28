_default:
    just --list

build profile="default":
    nix build \
        --json \
        --no-link \
        --print-build-logs \
        '.#{{ profile }}'

check:
    nix flake check

package profile="default":
    #!/usr/bin/env bash
    set -euxo pipefail
    DERIVATION=$(just _build "{{ profile }}")
    OUTPUT=$(echo $DERIVATION | jq -r .[0].outputs.out)
    install --mode 755 $OUTPUT/bin/vorpal-build .
    install --mode 755 $OUTPUT/bin/vorpal-cli .

update:
    nix flake update
