ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]')
DIST_DIR := ./dist
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
TARGET := ./target/release/vorpal
VORPAL_DIR := /vorpal
WORK_DIR := $(shell pwd)

# Development

test: build
	cargo test

build: check
	cargo build

check: lint
	cargo check

lint: format
	cargo clippy -- -D warnings

format:
	cargo fmt --check

clean:
	cargo clean
	rm -rf $(DIST_DIR)

list:
	@grep '^[^#[:space:]].*:' Makefile

# Release

dist: clean test-release
	mkdir -p $(DIST_DIR)
	cp $(TARGET) $(DIST_DIR)/vorpal
	tar -czvf "vorpal-$(ARCH)-$(OS).tar.gz" -C $(DIST_DIR) vorpal

test-release: build-release
	cargo test --release

build-release: check-release
	cargo build --release

check-release: lint
	cargo check --release

# Sandbox rootfs

build-docker:
	docker buildx build --load --progress="plain" --tag "altf4llc/vorpal-rootfs:latest" .

export-docker: build-docker
	docker container create --name 'vorpal-rootfs' 'altf4llc/vorpal-rootfs:latest'
	mkdir -pv $(DIST_DIR)
	docker export 'vorpal-rootfs' | zstd -v > $(DIST_DIR)/vorpal-rootfs.tar.zst
	docker container rm --force 'vorpal-rootfs'
	rm -rf $(VORPAL_DIR)/sandbox-rootfs
	mkdir -p $(VORPAL_DIR)/sandbox-rootfs
	tar -xvf $(DIST_DIR)/vorpal-rootfs.tar.zst -C $(VORPAL_DIR)/sandbox-rootfs
	echo 'nameserver 1.1.1.1' > $(VORPAL_DIR)/sandbox-rootfs/etc/resolv.conf

# Development virtual environments

build-packer: validate-packer
	rm -rf $(WORK_DIR)/packer_$(OS_TYPE)_vmware_arm64.box
	packer build \
		-var-file=$(WORK_DIR)/.packer/pkrvars/$(OS_TYPE)/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer

load-vagrant: build-packer
	vagrant box remove --force "altf4llc/debian-bookworm" || true
	vagrant box add \
		--name "altf4llc/debian-bookworm" \
		--provider "vmware_desktop" \
		$(WORK_DIR)/packer_debian_vmware_arm64.box

test-vagrant:
	vagrant destroy --force || true
	vagrant up --provider "vmware_desktop"

validate-packer:
	packer validate \
		-var-file=$(WORK_DIR)/.packer/pkrvars/$(OS_TYPE)/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer
