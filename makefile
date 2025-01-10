ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
DIST_DIR := $(WORK_DIR)/dist
TARGET ?= debug
VORPAL_DIR := /var/lib/vorpal
CARGO_FLAGS := $(if $(filter $(TARGET),release),--release,)

ifndef VERBOSE
.SILENT:
endif

.DEFAULT_GOAL := build

# Development (without Vorpal)

clean-cargo:
	cargo clean

clean-dist:
	rm -rfv $(DIST_DIR)

clean-vagrant:
	vagrant destroy --force

clean: clean-cargo clean-dist clean-vagrant

check:
	cargo check $(CARGO_FLAGS)

format:
	cargo fmt --check

lint:
	cargo clippy -- -D warnings

build:
	cargo build $(CARGO_FLAGS)

test:
	cargo test $(CARGO_FLAGS)

dist: build
	mkdir -pv $(DIST_DIR)
	tar -czvf $(DIST_DIR)/vorpal-$(ARCH)-$(OS).tar.gz \
		-C $(WORK_DIR)/target/$(TARGET) vorpal

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
	vagrant up --provider "vmware_desktop"
