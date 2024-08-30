_default:
    just --list

# build everything
build args="":
    cargo build -j $(nproc) --package "vorpal-cli" {{ args }}

# build sandbox (docker)
build-docker-sandbox:
    docker buildx build \
        --file "Dockerfile.sandbox" \
        --tag "ghcr.io/alt-f4-llc/vorpal-sandbox:edge" \
        .

# build (docker)
build-docker: build-docker-sandbox
    docker buildx build \
        --tag "ghcr.io/alt-f4-llc/vorpal:edge" \
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

run-docker +command: build-docker
    docker container run \
        --env "NICKEL_IMPORT_PATH=${PWD}/.vorpal/packages:${PWD}" \
        --interactive \
        --network "host" \
        --rm \
        --tty \
        --volume "${PWD}:${PWD}" \
        --volume "/var/lib/vorpal:/var/lib/vorpal" \
        --workdir "${PWD}" \
        ghcr.io/alt-f4-llc/vorpal:edge \
        {{ command }}

# start (cargo)
start:
    cargo run --package "vorpal-cli" -- worker start

# start (docker)
start-docker: build-docker
    docker container run \
        --detach \
        --publish "127.0.0.1:23151:23151" \
        --rm \
        --tty \
        --volume "/var/lib/vorpal:/var/lib/vorpal" \
        --volume "/var/run/docker.sock:/var/run/docker.sock" \
        ghcr.io/alt-f4-llc/vorpal:edge \
        worker start

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
