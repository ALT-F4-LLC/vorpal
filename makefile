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
# The SDK test legs share CARGO_FLAGS' hermeticity guarantee: under
# TARGET=release they resolve every dependency from an already-warm cache and
# fail loudly instead of reaching the network. `make test-sdk-warm` is the one
# deliberate networked seam that fills those caches, the way `make vendor`
# fills cargo's; on a warm cache it too touches no network.
GO_TEST_ENV := $(if $(filter $(TARGET),release),GOFLAGS=-mod=readonly GOPROXY=off,)
BUN_INSTALL_FLAGS := --frozen-lockfile$(if $(filter $(TARGET),release), --offline,)
# `system services start` refuses a worker/registry with no issuer, so
# vorpal-start and lima-vorpal-start must name one. This development default
# is the realm `vorpal login`'s clap default names and the realm
# script/test/keycloak.sh's clients live in (KC_REALM=vorpal), so all three
# agree. Nothing in this repository *provisions* that realm: `docker compose
# up` boots Keycloak with only its own `master` realm, and the `vorpal` realm
# and its OIDC clients are still created by hand. `realms/master` was a prior
# default and is worse, not simpler — it is Keycloak's administrative realm,
# holds none of Vorpal's clients, and ships with a published admin/password
# bootstrap credential, so it satisfies "an issuer is configured" while
# pointing a running service at the wrong realm.
#
# A Rust test (`makefile_default_vorpal_issuer` in cli/src/command.rs) reads
# the line below at compile time and fails if it drifts from the CLI's own
# default, so edit both together.
VORPAL_ISSUER ?= http://localhost:8080/realms/vorpal

# Guard for VORPAL_ISSUER, expanded by the two recipes that interpolate it.
#
# Make substitutes the value as plain text into two different shell contexts
# before either shell parses the line, so the deny-list is the union of what
# each can reinterpret: vorpal-start puts it in a double-quoted string ('"',
# '`', '$', '\'), and lima-vorpal-start nests that inside `bash -c '...'`,
# where an apostrophe ends the outer quoting and injects into the VM.
#
# Reads with `$(value VORPAL_ISSUER)`, not `$(VORPAL_ISSUER)`: the variable is
# recursively expanded, so a plain reference re-expands its text on every use
# — including any `$(shell ...)` the operator's value contains — before
# `findstring` ever sees it. `make VORPAL_ISSUER='$(shell touch pwned)'` ran
# the shell command as a side effect of the check meant to refuse it, with
# the deny-list never seeing a literal `$`. `$(value ...)` returns the raw
# text unexpanded.
#
# Expanded inside the two recipes rather than once at file scope, so a
# malformed value fails only the targets that would interpolate it instead of
# aborting every target in this file, `make build` included.
#
# Out of reach either way: `make VORPAL_ISSUER:=...` on the command line is a
# simply-expanded assignment, which Make expands at assignment time — before
# any part of this file runs — so `$(value ...)` sees the result and no check
# here can prevent it. Nothing in this file can; the guard covers the
# recursively-expanded forms, which are the ones an operator writes.
CHECK_VORPAL_ISSUER = $(if $(strip $(findstring ",$(value VORPAL_ISSUER))$(findstring ',$(value VORPAL_ISSUER))$(findstring `,$(value VORPAL_ISSUER))$(findstring \,$(value VORPAL_ISSUER))$(findstring $$,$(value VORPAL_ISSUER))),$(error VORPAL_ISSUER may not contain a quote, an apostrophe, a backtick, a backslash, or '$$'))

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

test-sdk-warm:
	cd sdk/go && go mod download
	cd sdk/typescript && bun install --frozen-lockfile

test-sdk-go:
	cd sdk/go && $(GO_TEST_ENV) go test -race -count=1 ./...

test-sdk-typescript-install:
	cd sdk/typescript && bun install $(BUN_INSTALL_FLAGS)

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
# that list maps onto `format`, `build` and `test` above).
#
# `secret-scan`, `ac-commands` and `doc-record` below are real. The remaining
# two, `sdet-abuse` and `vuln-scan`, are still STUBS: each exits 0 without
# performing the check its name implies, so a workflow gating on either is not
# checking anything. Their trust entries carry `stub(no-real-check: ...)`,
# which makes docket mark their passes hollow in run reports rather than
# counting them as real coverage.
#
# Making a target real here is only half the change: the trust entry's stub
# annotation is operator state, cleared by re-running `docket trust add` for
# that gate. Until that happens the three real targets still report as hollow
# passes.

STUB = echo "STUB: 'make $@' performed no check (see 'Docket gates' in makefile)"

# ac-commands — run this repo's own checks and report each result.
#
# Wired as a pre-gate on `verify`, where the output lands in the step's context
# bundle. `verify-ac` judges acceptance criteria against the diff, and an AC of
# the form "the tests pass" is one it cannot settle by reading code: without
# recorded command results it must answer `unverifiable`, which parks the step.
#
# The exit status is the WORST any check recorded, never the sum: summed exits
# wrap at 256 back to 0, reporting a failure as success. A failing pre-gate
# does not refuse the claim; the engine carries the failure into the bundle as
# data, so the verifier sees an honest verdict and the step still runs.
#
# KNOWN LIMIT: this cannot run the fenced AC commands harvested per issue. A
# gate process receives an allowlisted environment with no run, issue or step
# identity, so it cannot locate the issue whose fences it would execute.
# Closing that needs the engine to pass step identity into the gate.
ac-commands:
	echo "=== ac-commands: recorded results for the verifier ==="
	echo "commit: $$(git rev-parse --short HEAD)"
	echo "tree:   $$(git status --porcelain | wc -l | tr -d ' ') uncommitted path(s)"
	worst=0; \
	log=$$(mktemp) || exit 1; \
	trap 'rm -f "$$log"' EXIT; \
	for target in build test; do \
		echo "--- $$target: make $$target ---"; \
		status=0; \
		$(MAKE) $$target >"$$log" 2>&1 || status=$$?; \
		tail -20 "$$log"; \
		echo "[$$target] exit $$status"; \
		if [ "$$status" -gt "$$worst" ]; then worst=$$status; fi; \
		echo; \
	done; \
	echo "=== end ac-commands ==="; \
	exit $$worst

# doc-record — the trusted action behind the spec-doc workflow's record step.
#
# Not a check: it reads a JSON context bundle on stdin and writes exactly one
# JSON document back on stdout, so a single stray byte there fails the step.
# Silenced with `@` for that reason, and shipped by the docket corpus rather
# than by this repository.
doc-record:
	@"$$HOME/.docket/bin/doc-record"

sdet-abuse:
	$(STUB)

# secret-scan — refuse a change that adds a credential to the tree.
#
# Scope is what THIS step added. A secret already in an older commit is not
# this step's doing, and failing on it would make the gate unclearable for
# every later step that touches the file.
#
# FOUR sources, because a change hides in any of them:
#
#   staged     this gate also runs before a commit, and a staged file is
#              absent from both a bare working-tree diff and the untracked
#              enumeration — it would fall straight through the gap.
#   unstaged   the ordinary in-progress edit.
#   untracked  a brand-new file is exactly where a credential lands, and
#              `git diff` cannot see one.
#   HEAD       LOAD-BEARING, and the reason a three-source scan would be
#              decorative here. The engine re-runs gates after the step's
#              hand-back commit, against a clean tree: all three sources above
#              are empty by then, so the gate would report "nothing to scan"
#              on every invocation it actually performs. HEAD's own patch is
#              the change under review at that point.
#
# KNOWN LIMIT of the HEAD source: it is one commit. A step that hands back
# several commits has only its last one scanned this way (the earlier ones
# were covered while they were still staged or unstaged, on an earlier run of
# this gate, which is weaker evidence than scanning the range).
#
# FAILS CLOSED on a git that refuses. Every line scanned comes from git, so a
# git that errors leaves the input empty — and an empty scan reads as "clean",
# a security gate passing precisely when it inspected nothing. Each git
# invocation's own exit status is therefore checked. This is not `pipefail`
# territory: make runs recipes under /bin/sh, where a pipeline's status is its
# LAST command's, so a `git ... | grep` would report grep's success and hide
# the failure. Hence the redirect-then-grep shape below.
#
# The matching line is never printed: it would land in the gate's recorded
# verdict, the event log and every transcript quoting them, making the gate
# itself the leak. The count is enough to go looking.
#
# Untracked names reach `sh` as ARGUMENTS, never spliced into its command
# string. A file named `x"; rm -rf ~; "` would otherwise execute as code in
# the gate's own context — a scanner that runs what it is meant to inspect.
#
# Patterns are high-signal shapes only. One that fires on ordinary code trains
# people to route around the gate, so the bar is that a true positive is far
# likelier than a false one. `github_pat_` is listed separately from
# `gh[pousr]_` because fine-grained tokens use a different prefix entirely.
SECRET_PATTERNS := AKIA[0-9A-Z]{16}|gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{40,}|xox[abprs]-[0-9A-Za-z-]{10,}|sk-ant-[A-Za-z0-9_-]{20,}|AIza[0-9A-Za-z_-]{35}|-----BEGIN [A-Z ]*PRIVATE KEY-----

SECRET_SCAN_COLLECT_FAILED = echo "secret-scan FAILED: could not collect the change set; nothing was scanned." >&2; exit 1

secret-scan:
	if ! git rev-parse --git-dir >/dev/null 2>&1; then \
		echo "secret-scan FAILED: not a git repository, so nothing can be scanned." >&2; \
		exit 1; \
	fi
	raw=$$(mktemp) || exit 1; \
	scanned=$$(mktemp) || exit 1; \
	list=$$(mktemp) || exit 1; \
	trap 'rm -f "$$raw" "$$scanned" "$$list"' EXIT; \
	git diff --cached --unified=0 -- . >"$$raw" || { $(SECRET_SCAN_COLLECT_FAILED); }; \
	git diff --unified=0 -- . >>"$$raw" || { $(SECRET_SCAN_COLLECT_FAILED); }; \
	git show --unified=0 --format= --diff-merges=first-parent HEAD -- . >>"$$raw" || { $(SECRET_SCAN_COLLECT_FAILED); }; \
	/usr/bin/grep '^+' "$$raw" | /usr/bin/grep -v '^+++' >"$$scanned" || true; \
	git ls-files --others --exclude-standard -z >"$$list" || { $(SECRET_SCAN_COLLECT_FAILED); }; \
	xargs -0 sh -c 'for f; do if test -f "./$$f"; then cat "./$$f" || exit 1; fi; done' _ <"$$list" >>"$$scanned" || { $(SECRET_SCAN_COLLECT_FAILED); }; \
	if [ ! -s "$$scanned" ]; then \
		echo "secret-scan: no added lines to scan"; \
		exit 0; \
	fi; \
	hits=$$(/usr/bin/grep -cE '$(SECRET_PATTERNS)' "$$scanned" || true); \
	if [ "$$hits" -gt 0 ]; then \
		echo "secret-scan FAILED: $$hits added line(s) match a credential pattern." >&2; \
		echo "Remove it and re-run. If it is a fixture, make it unmistakably fake." >&2; \
		exit 1; \
	fi; \
	echo "secret-scan: clean"

vuln-scan:
	$(STUB)

# Development (with Vorpal)

vorpal-build:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- build $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-prepare:
	VORPAL_SOCKET_PATH=$(VORPAL_SOCKET) cargo $(CARGO_FLAGS) run --bin "vorpal" -- prepare $(VORPAL_FLAGS) $(VORPAL_ARTIFACT)

vorpal-start:
	$(CHECK_VORPAL_ISSUER)
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
	$(CHECK_VORPAL_ISSUER)
	limactl shell "vorpal-$(LIMA_ARCH)" bash -c '~/vorpal/target/debug/vorpal system services start --issuer "$(VORPAL_ISSUER)" $(VORPAL_FLAGS)'
