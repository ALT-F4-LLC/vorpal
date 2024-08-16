docker_build_cache := `echo "$PWD/.buildx"`

_default:
    just --list

build:
    cargo check -j $(nproc)
    cargo build -j $(nproc)

build-docker tag="dev":
    docker buildx build \
        --cache-from "type=local,src={{ docker_build_cache }}" \
        --cache-to "type=local,dest={{ docker_build_cache }},mode=max" \
        --file "Dockerfile.sandbox" \
        --progress "plain" \
        --tag "vorpal-sandbox:{{ tag }}" \
        .

check:
    nix flake check

clean:
    rm -rf ./.buildx ./target

format:
    cargo fmt --check --package vorpal --verbose
    nix fmt -- --check .

generate:
    cargo run --package "vorpal-cli" keys generate

lint:
    cargo clippy -- -D warnings

package profile="default":
    nix build --json --no-link --print-build-logs ".#{{ profile }}"

start-worker:
    cargo run --package "vorpal-worker"

test:
    cargo test -j $(nproc)

update:
    cargo update
    nix flake update
