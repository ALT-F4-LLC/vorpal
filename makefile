ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
CARGO_DIR := $(WORK_DIR)/.cargo
DIST_DIR := $(WORK_DIR)/dist
VENDOR_DIR := $(WORK_DIR)/vendor
VORPAL_DIR := /var/lib/vorpal
TARGET ?= debug
CARGO_FLAGS := $(if $(filter $(TARGET),release),--offline --release,)
CONFIG_FILE ?= target/$(TARGET)/vorpal-config

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
		vorpal \
		vorpal-config

vendor:
	cargo vendor --versioned-dirs $(VENDOR_DIR)

# Vorpal

generate:
	rm -rfv sdk/go/api
	mkdir -pv sdk/go/api
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/api \
		--proto_path=crates/schema/api \
		v0/agent/agent.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/api \
		--proto_path=crates/schema/api \
		v0/artifact/artifact.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/api \
		--proto_path=crates/schema/api \
		v0/archive/archive.proto
	protoc \
		--go_opt=paths=source_relative \
		--go_out=sdk/go/api \
		--go-grpc_opt=paths=source_relative \
		--go-grpc_out=sdk/go/api \
		--proto_path=crates/schema/api \
		v0/worker/worker.proto

# Development (with Vorpal)

vorpal-example:
	"target/$(TARGET)/vorpal" artifact \
		--config "$(CONFIG_FILE)" \
		--name "vorpal-example"

vorpal-export:
	"target/$(TARGET)/vorpal" artifact \
		--config "$(CONFIG_FILE)" \
		--export \
		--name "vorpal"

vorpal-shell:
	"target/$(TARGET)/vorpal" artifact \
		--config "$(CONFIG_FILE)" \
		--name "vorpal-shell" \
		--path

vorpal:
	"target/$(TARGET)/vorpal" artifact \
		--config "$(CONFIG_FILE)" \
		--name "vorpal"

vorpal-start:
	"target/$(TARGET)/vorpal" start

vorpal-config-start:
	"$(CONFIG_FILE)" start --artifact "$(ARTIFACT)" --port "50051"

# Vagrant environment

vagrant-box:
	packer validate \
		-var-file=$(WORK_DIR)/.packer/pkrvars/$(OS_TYPE)/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer
	packer build \
		-var-file=$(WORK_DIR)/.packer/pkrvars/$(OS_TYPE)/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer
	vagrant box add \
		--name "altf4llc/debian-bookworm" \
		--provider "vmware_desktop" \
		$(WORK_DIR)/packer_debian_vmware_arm64.box

vagrant:
	vagrant destroy --force || true
	vagrant up --provider "vmware_desktop"
