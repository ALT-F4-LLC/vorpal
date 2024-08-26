_default:
    just --list

# build everything
build args="":
    cargo build -j $(nproc) --package "vorpal-cli" {{ args }}

# build (docker)
build-docker tag="edge":
    docker buildx build \
        --file "Dockerfile" \
        --tag "ghcr.io/alt-f4-llc/vorpal:{{ tag }}" \
        .
    docker buildx build \
        --file "Dockerfile.sandbox" \
        --tag "ghcr.io/alt-f4-llc/vorpal-sandbox:{{ tag }}" \
        .

# check (cargo)
check args="":
    cargo check -j $(nproc) {{ args }}

# clean everything
clean:
    cargo clean

# format cargo
format:
    cargo fmt --check --verbose

# lint
lint:
    cargo clippy -- -D warnings

# start (worker)
start:
    cargo run --package "vorpal-cli" -- worker start

# test cargo
test-cargo args="":
    cargo test -j $(nproc) {{ args }}

# test nickel
test-nickel system="x86_64-linux":
    #!/usr/bin/env bash
    set -euo pipefail
    tmpfile="vorpal.test.ncl"
    trap 'rm -f "$tmpfile"' EXIT
    echo 'let config = import "vorpal.ncl" in config "{{ system }}"' > $tmpfile
    nickel export $tmpfile

# test everything
test args: (test-cargo args) test-nickel

# update (cargo)
update:
    cargo update
