ARCH := $(shell uname -m | tr '[:upper:]' '[:lower:]')
OS := $(shell uname -s | tr '[:upper:]' '[:lower:]')
OS_TYPE ?= debian
WORK_DIR := $(shell pwd)
DIST_DIR := $(WORK_DIR)/dist
TARGET ?= debug
VORPAL_DIR := /vorpal
CARGO_FLAGS := $(if $(filter $(TARGET),release),--release,)

.DEFAULT_GOAL := build

# Development

clean-cargo:
	cargo clean

clean-dist:
	rm -rfv $(DIST_DIR)

clean-rootfs:
	docker container rm --force vorpal-rootfs-export
	rm -rfv $(WORK_DIR)/rootfs

clean: clean-cargo clean-dist clean-rootfs

check:
	cargo check $(CARGO_FLAGS)

format:
	cargo fmt --check

lint:
	cargo clippy -- -D warnings

build:
	cargo build $(CARGO_FLAGS)

build-rootfs:
	mkdir -pv $(WORK_DIR)/rootfs
	docker buildx build --load --progress=plain --tag 'altf4llc/vorpal-rootfs:latest' .
	docker container create --name vorpal-rootfs-export 'altf4llc/vorpal-rootfs:latest'
	docker export vorpal-rootfs-export | gzip -v > $(WORK_DIR)/rootfs/$(ARCH)-export.tar.gz
	mkdir -pv $(WORK_DIR)/rootfs/$(ARCH)
	tar -xvf $(WORK_DIR)/rootfs/$(ARCH)-export.tar.gz -C $(WORK_DIR)/rootfs/$(ARCH)
	echo 'nameserver 1.1.1.1' > $(WORK_DIR)/rootfs/$(ARCH)/etc/resolv.conf

test:
	cargo test $(CARGO_FLAGS)

dist: clean-cargo build
	mkdir -pv $(DIST_DIR)
	tar -czvf $(DIST_DIR)/vorpal-$(ARCH)-$(OS).tar.gz \
		-C $(WORK_DIR)/target/$(TARGET) vorpal vorpal-config

dist-rootfs: clean-rootfs build-rootfs
	mkdir -pv $(DIST_DIR)
	tar -czvf $(DIST_DIR)/vorpal-rootfs-$(ARCH).tar.gz \
		-C $(WORK_DIR)/rootfs/$(ARCH) --strip-components=1 .

# Development (virtual)

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
