_default:
    just --list

# build everything
build:
    cargo build -j $(nproc) --package "vorpal-cli"

# build (docker)
build-docker tag="edge":
    docker buildx build \
        --file "Dockerfile" \
        --platform "linux/amd64,linux/arm64" \
        --tag "ghcr.io/alt-f4-llc/vorpal:{{ tag }}" \
        .
    docker buildx build \
        --file "Dockerfile.sandbox" \
        --tag "ghcr.io/alt-f4-llc/vorpal-sandbox:{{ tag }}" \
        .

# check (cargo)
check:
    cargo check -j $(nproc)

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
test-cargo:
    cargo test -j $(nproc)

# test nickel
test-nickel system="x86_64-linux":
    nickel export <<< 'let config = import "vorpal.ncl" in config "{{ system }}"'

# test everything
test: test-cargo test-nickel

# update (cargo)
update:
    cargo update
