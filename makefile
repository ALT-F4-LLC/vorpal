ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
CARGO_DIR := $(WORK_DIR)/.cargo
DIST_DIR := $(WORK_DIR)/dist
VENDOR_DIR := $(WORK_DIR)/vendor
VORPAL_ARTIFACT := vorpal
VORPAL_DIR := /var/lib/vorpal
VORPAL_NAMESPACE := library
VORPAL_SOCKET := /tmp/vorpal-$(notdir $(WORK_DIR)).sock
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
	tar -czf $(DIST_DIR)/vorpal-$(ARCH)-$(OS).tar.gz \
		-C $(WORK_DIR)/target/$(TARGET) \
		vorpal

vendor:
	cargo vendor --versioned-dirs $(VENDOR_DIR)

# Vorpal

generate:
	rm -rf sdk/go/pkg/api
	mkdir -p sdk/go/pkg/api
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
		context/context.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/pkg/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/pkg/api \
		--proto_path=sdk/rust/api \
		worker/worker.proto
	rm -rf sdk/typescript/src/api
	mkdir -p sdk/typescript/src/api
	protoc \
		--plugin=protoc-gen-ts_proto=sdk/typescript/node_modules/.bin/protoc-gen-ts_proto \
		--ts_proto_out=sdk/typescript/src/api \
		--ts_proto_opt=outputServices=grpc-js \
		--ts_proto_opt=esModuleInterop=true \
		--ts_proto_opt=snakeToCamel=false \
		--ts_proto_opt=forceLong=number \
		--ts_proto_opt=useOptionals=messages \
		--ts_proto_opt=oneof=unions \
		--ts_proto_opt=env=node \
		--ts_proto_opt=importSuffix=.js \
		--proto_path=sdk/rust/api \
		agent/agent.proto artifact/artifact.proto archive/archive.proto context/context.proto worker/worker.proto
	cargo run -p linux-vorpal-codegen

# Development (with Vorpal)

vorpal:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-start:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- system services start $(VORPAL_FLAGS)

vorpal-website-start:
	bun run --cwd=website dev

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
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c 'cd ~/vorpal && target/debug/vorpal build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)'

lima-vorpal-start:
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c '~/vorpal/target/debug/vorpal system services start $(VORPAL_FLAGS)'
