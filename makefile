ARCH := $(shell bash script/platform.sh arch)
OS := $(shell bash script/platform.sh os)
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
	bash script/cargo-config.sh "$(CARGO_DIR)" "$(VENDOR_DIR)"

clean:
	bash script/clean.sh "$(CARGO_DIR)" "$(DIST_DIR)" "$(VENDOR_DIR)"

check:
	cargo check $(CARGO_FLAGS)

format:
	cargo fmt --all --check

lint:
	cargo clippy $(CARGO_FLAGS) --all-targets -- --deny warnings

build:
	cargo build $(CARGO_FLAGS)

test:
	cargo test $(CARGO_FLAGS)

dist:
	bash script/dist.sh "$(DIST_DIR)" "$(WORK_DIR)" "$(ARCH)" "$(OS)" "$(TARGET)"

vendor:
	cargo vendor --versioned-dirs $(VENDOR_DIR)

# Vorpal

generate:
	bash script/generate.sh

# Docket gates
#
# Each target backs the `docket trust` entry of the same name. The `vuln-scan`
# and `sdet-abuse` trust entries still carry `stub(no-real-check: ...)`, so
# docket marks their passes hollow until the operator re-adds them. `vuln-scan`
# needs `--network api.osv.dev` on the re-add; `sdet-abuse` needs `--tree` and a
# `--timeout`.

# Runs `build` and `test` and reports each result, as a pre-gate on `verify`.
ac-commands:
	MAKE="$(MAKE)" bash script/qa/ac-commands.sh

# Docket-shipped helper behind the spec-doc record step. Silenced with `@`
# because it must write exactly one JSON document to stdout.
doc-record:
	@"$$HOME/.docket/bin/doc-record"

# security-change's QA gate; runs the same suite as the `tests` gate.
qa-test: test

# Checks that changed docs carry this repo's header shape.
doc-validate:
	bash script/qa/doc-validate.sh

# The light tracks bind on a label alone; these refuse a change that outgrew
# its label. The `diff-scope-docs` trust entry maps to the script's docs-only track.
diff-scope-docs:
	bash script/qa/diff-scope.sh docs-only

diff-scope-trivial:
	bash script/qa/diff-scope.sh trivial

diff-scope-small:
	bash script/qa/diff-scope.sh small

.PHONY: qa-test doc-validate diff-scope-docs diff-scope-trivial diff-scope-small

# Abuse-case tests: Rust tests whose path contains `abuse` must exist and pass.
sdet-abuse:
	CARGO_FLAGS="$(CARGO_FLAGS)" bash script/qa/sdet-abuse.sh

# Refuses a change that adds a credential.
secret-scan:
	bash script/qa/secret-scan.sh

# Queries every tracked lockfile against OSV; scoped by DOCKET_GATE_BASE when set.
vuln-scan:
	bash script/qa/vuln-scan.sh

# Development (with Vorpal)

vorpal-build:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-prepare:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- prepare $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-start:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- system services start $(VORPAL_FLAGS)

vorpal-website-start:
	bun run --cwd=website dev

# Lima environment

lima-clean:
	bash script/lima-host.sh clean "$(LIMA_ARCH)"

lima: lima-clean
	bash script/lima-host.sh create "$(LIMA_ARCH)" "$(LIMA_CPUS)" "$(LIMA_DISK)" "$(LIMA_MEMORY)" "$(WORK_DIR)"

lima-sync:
	limactl shell "vorpal-$(LIMA_ARCH)" ./script/lima.sh sync

lima-vorpal:
	limactl shell "vorpal-$(LIMA_ARCH)" ./script/lima.sh vorpal-build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

lima-vorpal-start:
	limactl shell "vorpal-$(LIMA_ARCH)" ./script/lima.sh vorpal-start $(VORPAL_FLAGS)
