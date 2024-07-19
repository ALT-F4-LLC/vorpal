_default:
    just --list

# build (cargo)
build:
    #!/usr/bin/env bash
    set -euxo pipefail
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
    cargo check
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
    #!/usr/bin/env bash
    set -euxo pipefail
    nix build --json --no-link --print-build-logs ".#{{ profile }}" | jq -r .[0].outputs.out

start-agent workers: build
    cargo run services agent --workers "{{ workers }}"

start-worker: build
    cargo run services worker

# test (cargo)
test:
    cargo test

up: build-image-sandbox
    docker compose up --build --detach

# update flake (nix)
update:
    cargo update
    nix flake update
