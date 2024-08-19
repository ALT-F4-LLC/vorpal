_default:
    just --list

# build cli (only)
build-cli:
    cargo build -j $(nproc) --package "vorpal-cli"

# build sandbox (only)
build-sandbox tag="edge":
    docker buildx build \
        --file "Dockerfile" \
        --tag "ghcr.io/alt-f4-llc/vorpal-sandbox:{{ tag }}" \
        .

# build everything
build: build-cli build-sandbox

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

# start worker (only)
start-worker:
    cargo run --package "vorpal-worker"

# start everything
start: start-worker

# test cargo
test-cargo:
    cargo test -j $(nproc)

# test nickel
test-nickel:
    nickel export "vorpal.ncl"

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
