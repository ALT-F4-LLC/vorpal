ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
CARGO_DIR := $(WORK_DIR)/.cargo
DIST_DIR := $(WORK_DIR)/dist
VENDOR_DIR := $(WORK_DIR)/vendor
VORPAL_DIR := /var/lib/vorpal
TARGET ?= debug
CARGO_FLAGS := $(if $(filter $(TARGET),release),--release,)

ifndef VERBOSE
.SILENT:
endif

.DEFAULT_GOAL := build

# Development (without Vorpal)

clean-cargo:
	cargo clean

clean-dist:
	rm -rf $(DIST_DIR)

clean-vendor:
	rm -rf $(VENDOR_DIR)
	rm -rf $(CARGO_DIR)

clean: clean-cargo clean-dist clean-vendor

vendor:
	mkdir -p .cargo
	cargo vendor --versioned-dirs $(VENDOR_DIR) > $(CARGO_DIR)/config.toml

check:
	cargo --offline check $(CARGO_FLAGS)

format:
	cargo --offline fmt --all --check

lint:
	cargo --offline clippy -- --deny warnings

build:
	cargo --offline build $(CARGO_FLAGS)

test:
	cargo --offline test $(CARGO_FLAGS)

dist:
	mkdir -pv $(DIST_DIR)
	tar -czvf $(DIST_DIR)/vorpal-$(ARCH)-$(OS).tar.gz \
		-C $(WORK_DIR)/target/$(TARGET) \
		vorpal \
		vorpal-config

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

generate-toolkits:
	cat /var/lib/vorpal/store/8ad451bdcda8f24f4af59ccca23fd71a06975a9d069571f19b9a0d503f8a65c8.json \
		> crates/cli/src/toolkit/aarch64-darwin/protoc.json
	cat /var/lib/vorpal/store/84707c7325d3a0cbd8044020a5256b6fd43a79bd837948bb4a7e90d671c919e6.json \
		> crates/cli/src/toolkit/aarch64-darwin/rust-toolchain.json
	cat /var/lib/vorpal/store/8372cc49eb6d38aa86080493c58a09bbb74da56d771938770f3d4cec593a5260.json \
		> crates/cli/src/toolkit/aarch64-darwin/cargo.json
	cat /var/lib/vorpal/store/345ef03ddf58536389f1a915f8b4bbf21e8b529ac288fbe3ae897986ddf1807f.json \
		> crates/cli/src/toolkit/aarch64-darwin/clippy.json
	cat /var/lib/vorpal/store/4bd3745cd87cc821da649df3f115cf499344f8dda3c6c7fd9b291a17752d0d88.json \
		> crates/cli/src/toolkit/aarch64-darwin/rust-analyzer.json
	cat /var/lib/vorpal/store/b6a0e429fe2619118ac130ddb3399195b0331aab404d2fb4506635e17791ff32.json \
		> crates/cli/src/toolkit/aarch64-darwin/rust-src.json
	cat /var/lib/vorpal/store/371b8c3b3e23b72b036bef9faf812e0508fb5902b04abf1b2b9d4c654ab3ba66.json \
		> crates/cli/src/toolkit/aarch64-darwin/rust-std.json
	cat /var/lib/vorpal/store/39c04b25f924f05473a07be53bce9eb8920e6a83daefa957941933b737ca0c6f.json \
		> crates/cli/src/toolkit/aarch64-darwin/rustc.json
	cat /var/lib/vorpal/store/f12b6581f5198f2f1a804538e059645af0590225d9bfac35d2ebd20ff47fbc09.json \
		> crates/cli/src/toolkit/aarch64-darwin/rustfmt.json

# Development (with Vorpal)

vorpal-config:
	cargo build --bin "vorpal-config"

vorpal-export: vorpal-config
	cargo run --bin "vorpal" -- artifact --export --name "vorpal" > "vorpal-$(ARCH)-$(OS).json"

vorpal-shell: vorpal-config
	cargo run --bin "vorpal" -- artifact --name "vorpal-shell"

vorpal: vorpal-config
	cargo run --bin "vorpal" -- artifact --name "vorpal"

vorpal-start:
	cargo run --bin "vorpal" -- start

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
