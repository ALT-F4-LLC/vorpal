docker_build_cache := `echo "$PWD/.buildx"`

_default:
    just --list

# build (cargo)
build:
    cargo build --package vorpal

# build image (docker)
build-image tag="dev":
    docker buildx build \
        --cache-from "type=local,src={{ docker_build_cache }}" \
        --cache-to "type=local,dest={{ docker_build_cache }},mode=max" \
        --tag "vorpal:{{ tag }}" \
        .

# build image sandbox (docker)
build-image-sandbox tag="dev":
    docker buildx build \
        --cache-from "type=local,src={{ docker_build_cache }}" \
        --cache-to "type=local,dest={{ docker_build_cache }},mode=max" \
        --file "Dockerfile.sandbox" \
        --tag "vorpal-sandbox:{{ tag }}" \
        .

# check flake (nix)
check:
    nix flake check

# clean environment
clean: stop-docker
    rm -rf ./.buildx ./target

# format code (cargo & nix)
format:
    cargo fmt --check --package vorpal --verbose
    nix fmt -- --check .

generate: build
    cargo run keys generate

# lint code (cargo)
lint:
    cargo clippy -- -D warnings

logs service:
    docker container logs --follow --tail 100 "vorpal-{{ service }}"

# build and install (nix)
package profile="default":
    nix build --json --no-link --print-build-logs ".#{{ profile }}"

package-buildx-cache:
    tar --create --gzip --file buildx.tar.gz --verbose .buildx

start-agent workers: build
    sudo ./target/debug/vorpal services agent --workers "{{ workers }}"

start-worker: build
    sudo ./target/debug/vorpal services worker

# test (cargo)
test:
    cargo test

start-docker image="vorpal:dev" system="x86_64-linux": build-image build-image-sandbox
    #!/usr/bin/env bash
    set -euxo pipefail
    docker network create "vorpal"
    docker container run \
        --detach \
        --interactive \
        --name "vorpal-worker" \
        --network "vorpal" \
        --rm \
        --tty \
        --volume "/var/lib/vorpal:/var/lib/vorpal" \
        --volume "/var/run/docker.sock:/var/run/docker.sock" \
        {{ image }} \
        services worker
    docker container run \
        --detach \
        --interactive \
        --name "vorpal-agent" \
        --network "vorpal" \
        --publish "127.0.0.1:15323:15323" \
        --rm \
        --tty \
        --volume "${PWD}:${PWD}" \
        --volume "/var/lib/vorpal:/var/lib/vorpal" \
        --volume "/var/run/docker.sock:/var/run/docker.sock" \
        {{ image }} \
        services agent --workers "{{ system }}=http://vorpal-worker:23151"

stop-docker:
    docker container rm --force "vorpal-agent"
    docker container rm --force "vorpal-worker"
    docker network rm --force "vorpal"

# update flake (nix)
update:
    nix flake update
