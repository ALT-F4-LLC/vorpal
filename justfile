_default:
    just --list

build:
    nix build --json --no-link --print-build-logs .

check:
    nix flake check

clean:
    rm -rf ~/.vorpal
    mkdir -p ~/.vorpal

format:
    cargo fmt --check --verbose
    nix fmt -- --check .

lint:
    cargo clippy

package:
    #!/usr/bin/env bash
    set -euxo pipefail
    OUTPUT=$(just build | jq -r .[0].outputs.out)
    install --mode 755 $OUTPUT/bin/vorpal .

start: clean
    nix run ".#start-dev"

test:
    cargo test

update:
    nix flake update
