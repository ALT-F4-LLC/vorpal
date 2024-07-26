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
        --tag "docker.io/altf4llc/vorpal:{{ tag }}" \
        .

# build image sandbox (docker)
build-image-sandbox tag="dev":
    docker buildx build \
        --cache-from "type=local,src={{ docker_build_cache }}" \
        --cache-to "type=local,dest={{ docker_build_cache }},mode=max" \
        --file "Dockerfile.sandbox" \
        --tag "altf4llc/vorpal-sandbox:{{ tag }}" \
        .

# check flake (nix)
check:
    nix flake check

# clean environment
clean: down
    rm -rf target

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

package-buildx-cache:
    tar --create --gzip --file buildx.tar.gz --verbose .buildx

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
