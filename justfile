_default:
    just --list

# build everything
build args="":
    cargo build -j $(nproc) --package "vorpal-cli" {{ args }}

# build (docker)
build-docker tag="edge":
    docker buildx build \
        --cache-from "ghcr.io/alt-f4-llc/vorpal-sandbox:{{ tag }}" \
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

run +flags="":
    cargo run --package "vorpal-cli" {{ if flags != "" { "--" } else { "" } }} {{ flags }}

# start (worker)
start:
    cargo run --package "vorpal-cli" -- worker start

# test cargo
test-cargo args="":
    cargo test -j $(nproc) {{ args }}

# test nickel
test-nickel system="aarch64-linux":
    #!/usr/bin/env bash
    set -euo pipefail
    tmpfile="vorpal.test.ncl"
    trap 'rm -f "$tmpfile"' EXIT
    echo 'let config = import "vorpal.ncl" in config "{{ system }}"' > $tmpfile
    nickel export $tmpfile

# test everything
test args="" system="aarch64-linux": (test-cargo args) (test-nickel system)

# update (cargo)
update:
    cargo update
