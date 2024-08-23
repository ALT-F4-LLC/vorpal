_default:
    just --list

# build everything
build: build-sandbox
    cargo build -j $(nproc) --package "vorpal-cli"

# build sandbox (only)
build-sandbox tag="edge":
    docker buildx build \
        --file "Dockerfile.sandbox" \
        --tag "ghcr.io/alt-f4-llc/vorpal-sandbox:{{ tag }}" \
        .

# check cargo
check-cargo:
    cargo check -j $(nproc)

# check nix
check-nix:
    nix flake check

# check everything
check: check-cargo check-nix

# clean everything
clean:
    rm -rf ./target

# format cargo
format-cargo:
    cargo fmt --check --verbose

# format nix
format-nix:
    nix fmt -- --check .

# format everything
format: format-cargo format-nix

# lint
lint:
    cargo clippy -- -D warnings

# package (nix)
package profile="default":
    nix build --json --no-link --print-build-logs ".#{{ profile }}"

# start (worker)
start:
    cargo run --package "vorpal-cli" -- worker start

# test cargo
test-cargo:
    cargo test -j $(nproc)

# test nickel
test-nickel system="x86_64-linux":
    nickel export <<< 'let config = import "vorpal.ncl" in config "{{ system }}"'

# test everything
test: test-cargo test-nickel

# update cargo
update-cargo:
    cargo update

# update nix
update-nix:
    nix flake update

# update everything
update: update-cargo update-nix
