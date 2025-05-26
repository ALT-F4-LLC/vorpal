ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
CARGO_DIR := $(WORK_DIR)/.cargo
DIST_DIR := $(WORK_DIR)/dist
VENDOR_DIR := $(WORK_DIR)/vendor
VORPAL_ARTIFACT := vorpal
VORPAL_DIR := /var/lib/vorpal
TARGET ?= debug
CARGO_FLAGS := $(if $(filter $(TARGET),release),--offline --release,)
LIMA_ARCH := $(ARCH)
LIMA_CPUS := 8
LIMA_DISK := 100
LIMA_MEMORY := 8

ifndef VERBOSE
.SILENT:
endif

.DEFAULT_GOAL := build

# Development (without Vorpal)

.cargo:
	mkdir -p $(CARGO_DIR)
	echo '[source.crates-io]' >> $(CARGO_DIR)/config.toml
	echo 'replace-with = "vendored-sources"' >> $(CARGO_DIR)/config.toml
	echo '[source.vendored-sources]' >> $(CARGO_DIR)/config.toml
	echo 'directory = "$(VENDOR_DIR)"' >> $(CARGO_DIR)/config.toml

clean:
	cargo clean
	rm -rf $(CARGO_DIR)
	rm -rf $(DIST_DIR)
	rm -rf $(VENDOR_DIR)

check:
	cargo check $(CARGO_FLAGS)

format:
	cargo fmt --all --check

lint:
	cargo clippy $(CARGO_FLAGS) -- --deny warnings

build:
	cargo build $(CARGO_FLAGS)

test:
	cargo test $(CARGO_FLAGS)

dist:
	mkdir -p $(DIST_DIR)
	tar -czvf $(DIST_DIR)/vorpal-$(ARCH)-$(OS).tar.gz \
		-C $(WORK_DIR)/target/$(TARGET) \
		vorpal

vendor:
	cargo vendor --versioned-dirs $(VENDOR_DIR)

# Vorpal

generate:
	rm -rfv sdk/go/pkg/api
	mkdir -pv sdk/go/pkg/api
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/pkg/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/pkg/api \
		--proto_path=sdk/rust/api \
		agent/agent.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/pkg/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/pkg/api \
		--proto_path=sdk/rust/api \
		artifact/artifact.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/pkg/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/pkg/api \
		--proto_path=sdk/rust/api \
		archive/archive.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/pkg/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/pkg/api \
		--proto_path=sdk/rust/api \
		worker/worker.proto

# Development (with Vorpal)

vorpal:
	cargo $(CARGO_FLAGS) run --bin "vorpal" -- artifact --name $(VORPAL_ARTIFACT) $(VORPAL_FLAGS)

vorpal-start:
	cargo $(CARGO_FLAGS) run --bin "vorpal" -- start $(VORPAL_FLAGS)

vorpal-config-start:
	cargo $(CARGO_FLAGS) run --bin "vorpal-config" -- start --artifact "$(VORPAL_ARTIFACT)" --port "50051" $(VORPAL_FLAGS)

# Lima environment

lima-clean:
	limactl stop "vorpal-$(LIMA_ARCH)" || true
	limactl delete "vorpal-$(LIMA_ARCH)" || true

lima: lima-clean
	cat lima.yaml | limactl create --arch "$(LIMA_ARCH)" --cpus "$(LIMA_CPUS)" --disk "$(LIMA_DISK)" --memory "$(LIMA_MEMORY)" --name "vorpal-$(LIMA_ARCH)" -
	limactl start "vorpal-$(LIMA_ARCH)"
	limactl shell "vorpal-$(LIMA_ARCH)" $(WORK_DIR)/script/lima.sh install
	limactl stop "vorpal-$(LIMA_ARCH)"
	limactl start "vorpal-$(LIMA_ARCH)"

lima-sync:
	limactl shell "vorpal-$(LIMA_ARCH)" ./script/lima.sh sync

lima-vorpal:
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c '~/vorpal/target/debug/vorpal artifact --name $(VORPAL_ARTIFACT) $(VORPAL_FLAGS)'

lima-vorpal-start:
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c '~/vorpal/target/debug/vorpal start $(VORPAL_FLAGS)'
