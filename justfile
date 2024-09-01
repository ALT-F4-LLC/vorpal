_default:
    just --list

# build
build args="":
    cargo build --package "vorpal-cli" {{ args }}

# check
check args="":
    cargo check {{ args }}

# clean
clean:
    cargo clean

# format
format:
    cargo fmt --check --verbose

# lint
lint:
    cargo clippy -- -D warnings

# run
run +flags="":
    cargo run --package "vorpal-cli" {{ if flags != "" { "--" } else { "" } }} {{ flags }}

# start worker
start-worker:
    cargo run --package "vorpal-cli" -- worker start

# test nickel
test-nickel system="aarch64-linux":
    echo 'let config = import "vorpal.ncl" in config "{{ system }}"' | nickel export

# test everything
test args="" system="aarch64-linux": (test-nickel system)
    cargo test {{ args }}

# update cargo
update:
    cargo update
