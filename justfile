_default:
    just --list

# build (cargo)
build:
    cargo build --package vorpal

# build image (docker)
build-image tag="dev":
    #!/usr/bin/env bash
    set -euxo pipefail
    docker buildx build \
        --tag "altf4llc/vorpal-build:{{ tag }}" \
        --target "build" \
        .
    docker buildx build \
        --cache-from "altf4llc/vorpal-build:{{ tag }}" \
        --tag "altf4llc/vorpal:{{ tag }}" \
        .

# build sandbox image (docker)
build-image-sandbox tag="dev":
    #!/usr/bin/env bash
    set -euxo pipefail
    docker buildx build \
        --tag "altf4llc/vorpal-sandbox:{{ tag }}" \
        --target "sandbox" \
        .

# check flake (nix)
check:
    nix flake check

# clean environment
clean: down
    rm -rf target
    rm -rf /var/lib/vorpal/key
    rm -rf /var/lib/vorpal/sandbox
    rm -rf /var/lib/vorpal/store

down:
    docker compose down --remove-orphans --rmi=local --volumes

# format code (cargo & nix)
format:
    cargo fmt --check --package vorpal --verbose
    nix fmt -- --check .

generate: build
    cargo run keys generate

# lint code (cargo)
lint:
    cargo clippy -- -D warnings

logs:
    docker compose logs --follow

# build and install (nix)
package profile="default":
    nix build --json --no-link --print-build-logs ".#{{ profile }}"

start-agent workers: build
    sudo ./target/debug/vorpal services agent --workers "{{ workers }}"

start-worker: build
    sudo ./target/debug/vorpal services worker

# test (cargo)
test:
    cargo test

up: build-image-sandbox
    docker compose up --build --detach

# update flake (nix)
update:
    nix flake update
