ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]')
DIST_DIR := ./dist
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
TARGET := ./target/release/vorpal
WORK_DIR := $(shell pwd)

build-packer: validate-packer
	rm -rf $(WORK_DIR)/packer_debian_vmware_arm64.box
	packer build \
		-var-file=$(WORK_DIR)/.packer/pkrvars/debian/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer

build: clean check
	cargo build --release

check: lint
	cargo check --release

clean:
	cargo clean

dist: test
	mkdir -p $(DIST_DIR)
	cp $(TARGET) $(DIST_DIR)/vorpal
	tar -czvf "vorpal-$(ARCH)-$(OS).tar.gz" -C $(DIST_DIR) vorpal

format:
	cargo fmt --check

lint: format
	cargo clippy -- -D warnings

list:
	@grep '^[^#[:space:]].*:' Makefile

load-vagrant: build-packer
	vagrant box remove --force "altf4llc/debian-bookworm" || true
	vagrant box add \
		--name "altf4llc/debian-bookworm" \
		--provider "vmware_desktop" \
		$(WORK_DIR)/packer_debian_vmware_arm64.box

test: build
	cargo test --release

test-vagrant:
	vagrant destroy --force || true
	vagrant up --provider "vmware_desktop"

validate-packer:
	packer validate \
		-var-file=$(WORK_DIR)/.packer/pkrvars/debian/fusion-13.pkrvars.hcl \
		$(WORK_DIR)/.packer
