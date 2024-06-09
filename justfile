_default:
    just --list

# build cli (nix)
build:
    #!/usr/bin/env bash
    set -euxo pipefail
    cargo build --package vorpal
    install --mode 755 target/debug/vorpal .

# check flake (nix)
check:
    nix flake check

# clean environment
clean:
    rm -rf $HOME/.vorpal
    rm -rf example/rust/target
    rm -rf target
    rm -rf vorpal

# format code (cargo & nix)
format:
    cargo fmt --check --package vorpal --verbose
    nix fmt -- --check .

# lint code (cargo)
lint:
    cargo clippy

# build and install (nix)
package:
    #!/usr/bin/env bash
    set -euxo pipefail
    OUTPUT=$(nix build --json --no-link --print-build-logs . | jq -r .[0].outputs.out)
    install --mode 755 $OUTPUT/bin/vorpal .

# run service (cargo)
start service:
    cargo run --bin vorpal service {{ service }} start

# run all services (nix)
start-all:
    nix run . ".#start"

# test (cargo)
test:
    cargo test

# update flake (nix)
update:
    nix flake update
