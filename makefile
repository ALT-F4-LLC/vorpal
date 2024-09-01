ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
DIST_DIR := ./dist
TARGET := ./target/release/vorpal

build: check
	cargo build --release

check: lint
	cargo check --release

dist: test
	mkdir -p $(DIST_DIR)
	cp $(TARGET) $(DIST_DIR)/vorpal
	tar -czvf "vorpal-$(ARCH)-$(OS).tar.gz" -C $(DIST_DIR) vorpal
	rm -rf $(DIST_DIR)

format:
	cargo fmt --check

lint: format
	cargo clippy -- -D warnings

test: build
	cargo test --release
