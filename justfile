_default:
    just --list

# build cli (nix)
build:
    #!/usr/bin/env bash
    set -euxo pipefail
    cargo build --package vorpal

build-sandbox tag="ubuntu-24.04":
    #!/usr/bin/env bash
    set -euxo pipefail
    docker buildx build --tag "altf4llc/vorpal-sandbox:{{ tag }}" --target "sandbox" .

# check flake (nix)
check:
    cargo check
    nix flake check

# clean environment
clean: clean-cache
    rm -rf target
    rm -rf vorpal

# clean store cache
clean-cache:
    rm -rf /var/lib/vorpal/bundle/*
    rm -rf /var/lib/vorpal/image/*
    rm -rf /var/lib/vorpal/package/*
    rm -rf /var/lib/vorpal/source/*
    rm -rf /var/lib/vorpal/store/*

# format code (cargo & nix)
format:
    cargo fmt --check --package vorpal --verbose
    nix fmt -- --check .

# generate keys (cargo)
generate:
    cargo run --bin vorpal keys generate

# lint code (cargo)
lint:
    cargo clippy -- -D warnings

# build and install (nix)
package:
    #!/usr/bin/env bash
    set -euxo pipefail
    OUTPUT=$(nix build --json --no-link --print-build-logs . | jq -r .[0].outputs.out)
    install --mode 755 $OUTPUT/bin/vorpal .

stack-setup:
    orb -m "vorpal" sudo $PWD/script/setup_agent.sh

stack-create:
    orbctl create nixos "vorpal"

stack-delete:
    orbctl delete --force "vorpal"

stack-start:
    orbctl start "vorpal"

stack-stop:
    orbctl stop "vorpal"

# run agent (cargo)
start-agent workers: build
    sudo ./target/debug/vorpal services agent --workers "{{ workers }}"

# run worker (cargo)
start-worker: build
    sudo ./target/debug/vorpal services worker

# test (cargo)
test:
    cargo test

# update flake (nix)
update:
    cargo update
    nix flake update
