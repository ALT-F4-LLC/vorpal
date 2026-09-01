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
# system services start refuses a worker/registry with no issuer (VPL-434),
# so vorpal-start/lima-vorpal-start must name one. This default matches the
# realm `vorpal login`'s own clap default names (cli/src/command.rs) and the
# realm script/test/keycloak.sh's clients live in (KC_REALM=vorpal) —
# `realms/master` (a prior default) is Keycloak's own administrative realm,
# holds none of Vorpal's OIDC clients, and is provisioned with a published
# admin/password bootstrap credential, so it satisfied AC3/AC4's "an issuer
# is configured" in letter while a running service trusted the wrong realm
# (VPL-711-C8). `docker compose up` alone only creates `master`; a `vorpal`
# realm with Vorpal's OIDC clients is still provisioned by hand today.
VORPAL_ISSUER ?= http://localhost:8080/realms/vorpal

# VORPAL_ISSUER is substituted by Make as plain text into two different shell
# contexts before either shell parses the line, so the deny-list is the union
# of what each context can reinterpret: vorpal-start puts it in a
# double-quoted string ('"', '`', '$', '\'), and lima-vorpal-start nests that
# inside `bash -c '...'`, where the single quote ends the outer quoting and
# injects into the VM (VPL-711 CLUSTER-P, CLUSTER-S). Checked once, in pure
# Make (no subshell), so no recipe runs with an unsafe value — the default
# above is unaffected.
#
# Reads with `$(value VORPAL_ISSUER)`, not `$(VORPAL_ISSUER)` (VPL-711-C2):
# `VORPAL_ISSUER` is a recursively-expanded variable, so a plain `$(...)`
# reference re-expands its text on every use — including any `$(shell ...)`
# call the operator's value itself contains — *before* `findstring` ever
# gets to look at it. `make VORPAL_ISSUER='$(shell touch pwned)'` therefore
# ran the shell command as a side effect of the check meant to refuse it,
# with the deny-list never seeing a literal `$`. `$(value ...)` returns the
# variable's raw text without expanding it, so the same call site can
# search it for a `$` without that `$` ever being interpreted as a
# function call.
ifneq ($(strip $(findstring ",$(value VORPAL_ISSUER))$(findstring ',$(value VORPAL_ISSUER))$(findstring `,$(value VORPAL_ISSUER))$(findstring \,$(value VORPAL_ISSUER))$(findstring $$,$(value VORPAL_ISSUER))),)
$(error VORPAL_ISSUER may not contain a quote, an apostrophe, a backtick, a backslash, or '$$')
endif

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
	cargo clippy $(CARGO_FLAGS) --all-targets -- --deny warnings

build:
	cargo build $(CARGO_FLAGS)

test: test-sdk-go test-sdk-typescript
	cargo test $(CARGO_FLAGS)

test-sdk-go:
	cd sdk/go && go test -race -count=1 ./...

test-sdk-typescript-install:
	cd sdk/typescript && bun install --frozen-lockfile

test-sdk-typescript: test-sdk-typescript-install
	cd sdk/typescript && bun test

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
	rm -rf sdk/python/src/vorpal_sdk/api
	mkdir -p sdk/python/src/vorpal_sdk/api
	uv run --project sdk/python --frozen --group dev python -m grpc_tools.protoc \
		--grpc_python_out=sdk/python/src/vorpal_sdk/api \
		--proto_path=sdk/rust/api \
		--pyi_out=sdk/python/src/vorpal_sdk/api \
		--python_out=sdk/python/src/vorpal_sdk/api \
		agent/agent.proto \
		artifact/artifact.proto \
		archive/archive.proto \
		context/context.proto \
		worker/worker.proto
	uv run --project sdk/python --frozen --group dev \
		python sdk/python/script/fix_proto_imports.py sdk/python/src/vorpal_sdk/api
	cargo run -p vorpal-sdk-codegen

generate-check:
	cargo run -p vorpal-sdk-codegen -- --check

# Docket gates
#
# These targets back the `docket trust` entries of the same name (the rest of
# that list maps onto `format`, `build` and `test` above). Every target in this
# section is a STUB: it exits 0 without performing the check its name implies,
# so a workflow that gates on it is not actually checking anything. Each is
# registered with `docket trust add --stub`, which makes docket mark its passes
# hollow in run reports rather than counting them as real coverage. Replace a
# stub with the real check before treating its gate as meaningful.

STUB = echo "STUB: 'make $@' performed no check (see 'Docket gates' in makefile)"

ac-commands:
	$(STUB)

doc-record:
	$(STUB)

sdet-abuse:
	$(STUB)

secret-scan:
	$(STUB)

vuln-scan:
	$(STUB)

# Development (with Vorpal)

vorpal-build:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-prepare:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- prepare $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-start:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- system services start --issuer "$(VORPAL_ISSUER)" $(VORPAL_FLAGS)

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
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c '~/vorpal/target/debug/vorpal system services start --issuer "$(VORPAL_ISSUER)" $(VORPAL_FLAGS)'
