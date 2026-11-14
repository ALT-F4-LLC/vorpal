# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- **Source upgrades**: `script/source.sh` gains `check`, `stage`,
  `mirror-check`, `upload-cmd`, `provenance`, `pins`, `set-version`,
  `digest`, and `digest-fill`, driven by `script/source/registry.json` (every
  SDK artifact source with its upstream, declared checksum or signature
  authority, mirror key, and SDK pins) and `script/source/ledger.json` (the
  tracked LFS/BLFS books and each deviation). Staging verifies each download
  against its authority, fails closed, and writes a deterministic tree plus a
  manifest; uploads use `aws s3api put-object --if-none-match '*'`.
- **Book command tracking**: `script/source.sh book-diff` maps every block of
  the linux-vorpal stage scripts, and the step sequence in `linux_vorpal.rs`,
  to the LFS/BLFS sections it implements (`ledger.json` `book_sections`,
  `book_skipped`, `stage_blocks`) and keeps the reviewed commands of each
  mapped or skipped section under `script/source/book/` (extracted under the
  books' MIT License). On a book move it diffs every mapped section's
  commands beside its stage blocks and reports new, removed, moved, and
  changed skipped sections. `--accept` marks the changed and moved sections
  for review, and `--mark` records each review. `--check` and
  `sdk/python/tests/test_book_map.py` fail when a stage block changes or
  moves without review or the map is incomplete. The 45 mapped sections start
  unaudited; the first book move reviews them all. Nothing runs these checks
  automatically yet: `make test` and CI do not include them.
- **Rust toolchain drift test**: `sdk/codegen/tests/rust_toolchain_drift.rs`
  fails when any SDK's build-target Rust version differs from
  `rust-toolchain.toml`.
- **Supply-chain tests**: `sdk/python/tests/test_supply_chain.py` checks that
  the registry covers every `Vorpal.lock` source and matches every SDK pin,
  and checks `provenance.json` records for every mirrored file, not only
  cpython and uv, including their inline digests against `Vorpal.lock`.
  Two files are exempt by exact key, each with a reason in
  `registry.json`: the two UnZip patches BLFS prints no MD5 for.

### Changed

- **Rust toolchain**: cargo, clippy, rust-analyzer, rust-std, rustc,
  rustfmt, and rust-src 1.93.1 -> 1.100.0.
- **bun**: 1.3.10 -> 1.4.4.
- **cpython**: 3.13.14 -> 3.14.7 (python-build-standalone 20260929), the
  LFS book's Python. The Python SDK and the Python project template now
  require Python 3.14 (`requires-python = ">=3.14,<3.15"`); projects that
  cap below 3.14 no longer resolve against the Vorpal interpreter.
- **crane**: 0.21.1 -> 0.22.1.
- **gh**: 2.87.3 -> 2.104.0.
- **git**: 2.53.0 -> 2.56.1. The build passes `NO_RUST=1`, since git 2.55 and later build Rust code by default and the build has no Cargo.
- **go**: 1.26.0 -> 1.27.4, with the `go` directive of the Go SDK and the
  Go project template.
- **go-tools**: 0.42.0 -> 0.52.0.
- **grpcurl**: 1.9.3 -> 1.9.4.
- **nodejs**: 22.22.0 -> 26.13.1 (LTS).
- **pnpm**: 10.30.3 -> 12.21.0. pnpm 12 ships a `.tar.gz` holding the
  executable and its `dist` tree, so the artifact now unpacks both into
  `bin`, and its macOS targets are `darwin-arm64` and `darwin-x64`.
- **protoc**: 34.0 -> 36.2.
- **protoc-gen-go**: 1.36.11 -> 1.36.12.
- **protoc-gen-go-grpc**: 1.79.1 -> 1.85.0.
- **rsync**: 3.4.1 -> 3.5.2.
- **staticcheck**: 2026.1 -> 2026.2.2.
- **uv**: 0.10.11 -> 0.13.8.
- **Lima VM**: `lima.yaml` moves from Debian 12 (bookworm, 20250316-2053)
  to Debian 13 (trixie, 20261111-2659), with trixie/latest as the fallback.
- **Source keyrings**: `script/source/keys/` pins the bun, kernel.org
  autosigner, Node.js (active releasers), rsync, and Rust signing keys.

### Fixed

- `script/source.sh check` no longer reports cpython blocked when GitHub's
  API times out listing python-build-standalone releases: it reads them in
  pages of 10 and fails with the listing error instead.

## [0.5.0] - 2026-11-10

### Added

- **Prepared artifact size limit**: the agent rejects a prepared artifact
  whose encoded size exceeds 3932160 bytes (4 MiB, the default gRPC decode
  limit, less 256 KiB) with an `invalid_argument` error that names the
  artifact, instead of failing later when a receiver decodes it.

### Changed

- **Breaking — step secrets**: step secrets now use a sealed secret envelope
  format (RSA-OAEP with AES-256-GCM), bound to the artifact namespace and the
  secret name. Each secret is limited to 64 KiB (65536 bytes). A secret sealed
  in the older format fails with
  `secret was encrypted with an older format and must be prepared again`;
  prepare your artifacts again after upgrading.

- **Breaking — secret delivery**: secrets are delivered to a build step as
  files at `$VORPAL_SECRET_DIR/<NAME>` instead of environment variables. A
  step that references a secret (`$NAME` or `${NAME}`) in its arguments or
  environment fails. Secret names must match `[A-Za-z_][A-Za-z0-9_]*`, must
  not start with `VORPAL_`, and must not repeat within a step (no duplicates).

- **Breaking — secret values**: step secret values are carried as `bytes`. A
  value may hold any bytes, including binary data that is not valid UTF-8, and
  its file holds the value byte for byte.

- **Breaking — issuer client secret**: the `--issuer-client-secret` flag is
  removed. Pass the secret in a file instead, with
  `--issuer-client-secret-file` or `VORPAL_ISSUER_CLIENT_SECRET_FILE`.
  Existing service units that pass `--issuer-client-secret` break until the
  installer is run again.

- **Release metadata**: bump SDK packages, CLI/config crates, templates,
  install script, website, and documentation to `0.5.0`.

### Fixed

- **CLI builds**: report a missing registry archive when the registry returns
  `NotFound` inside the pull stream, so a build without outputs no longer
  logs nothing.

- **Website docs**: enable GitHub-flavored Markdown so MDX pages render
  tables, and fix highlighted lines in the SDK guide code samples.

## [0.4.1] - 2026-09-12

### Changed

- **Rust SDK**: remove internal `.clone()` usage from `sdk/rust` by
  changing `artifact::step::{bash, bwrap, shell, docker}` and
  `artifact::get_env_key` to borrow instead of taking owned values,
  `ConfigContext::add_artifact` to take the artifact by value, and
  `ConfigContext::run` to consume `self`. `ConfigContext::get_artifact_store`
  now returns a reference. These are breaking changes for callers of the
  published crate; update `let ctx = &mut get_context().await?;` to
  `let mut ctx = get_context().await?;` and pass `&mut ctx` to `.build(...)`.

- **Go SDK**: change `language.GetGOOS` and `language.GetGOARCH` to take the
  canonical system string instead of the `ArtifactSystem` enum, matching the
  string-based dispatch used elsewhere in the SDK. Callers holding an enum
  value can convert it first via `ConfigContext.GetTargetStr()` or the
  equivalent canonical-string accessor.

- **Cross-SDK**: add a `SYSTEMS` constant/export to the Rust, Go, Python, and
  TypeScript SDKs, listing all four supported systems
  (`aarch64-darwin`, `aarch64-linux`, `x86_64-darwin`, `x86_64-linux`) so
  callers no longer need to declare that list themselves. Every built-in
  artifact builder and the website's SDK examples now reference it instead of
  a local literal. Exposed as `vorpal_sdk::artifact::system::SYSTEMS` in
  Rust, `config.SYSTEMS` in Go, and a top-level `SYSTEMS` export in the
  Python and TypeScript packages.

### Fixed

- **CLI sockets**: honor `VORPAL_SOCKET_PATH` and explicit socket flags by
  detecting explicitly passed flags instead of comparing against the clap
  default, and derive registry and worker defaults from the default address.

- **Linux bootstrap**: pin the gcc pass 1 host C++ dialect to `gnu++17` so
  libcody's `u8` literals compile when the host compiler defaults to C++20.

### Changed

- **Release metadata**: bump SDK packages, CLI/config crates, templates,
  install script, and documentation to `0.4.1`.

## [0.4.0] - 2026-07-08

### Changed

- **SDK systems**: normalize systems handling to four-way parity across the
  Rust, Go, Python, and TypeScript SDKs with canonical-string system parsing,
  rejection of enum labels and `UNKNOWN_SYSTEM`, an unexported builder
  `systems` field, codegen and template alignment, and tests (#502).

- **Release metadata**: bump SDK packages, CLI/config crates, templates,
  install script, and documentation to `0.4.0`.

## [0.3.0] - 2026-07-06

### Added

- **Python SDK**: add a first-class Python SDK package with generated API
  bindings, artifact builders, context helpers, CLI helpers, and parity coverage
  alongside the Rust, Go, and TypeScript SDKs.

- **Python builds**: add `cpython`, `uv`, and Python language builders across
  SDKs so Python projects can use Vorpal's artifact workflow.

- **Python template**: add a `vorpal init` project template for Python with
  `pyproject.toml`, `uv.lock`, and starter source files.

- **Python documentation**: add a Python guide and SDK README covering
  installation, project setup, artifact definitions, and builder usage.

### Changed

- **CLI/build commands**: expand command handling, build configuration, lock
  handling, and start-agent plumbing for Python workflows.

- **SDK codegen**: update generation so Rust, Go, TypeScript, and Python
  artifact surfaces stay aligned from the same definitions.

- **Release metadata**: bump templates, SDK packages, website, CLI/config
  crates, and package metadata to `0.3.0`.

### Build

- **Vorpal workflows**: update lockfile and workflow plumbing for the `0.3.0`
  release path.

- **Install/docs**: refresh install script behavior and reference documentation
  for the expanded language support.

## [0.2.2] - 2026-06-22

### Fixed

- **linux-vorpal**: stage_05 hardcoded `cacert.pem` but the curl-cacert source
  downloads `cacert-{version}.pem`, causing a "No such file or directory" error
  at build time. Thread `curl_cacert_version` into stage_05 and reference the
  versioned filename across the Rust source and Go/TS SDK codegen arrays.

- **pnpm artifact**: correct source target paths in the pnpm artifact builder
  across all three SDKs (Rust, Go, TypeScript).

### Dependencies

- Bump `jsonwebtoken` to v10.4.0 (#476)
- Update `aws-sdk-rust` monorepo (#490)
- Update `google.golang.org/grpc` to v1.81.1 (#481)
- Update `rcgen` to v0.14.8 (#475)
- Routine Cargo lockfile maintenance and minor Rust crate updates (`uuid`
  v1.23.2 #495, `serde_json` v1.0.150 #494, `http` v1.4.1 #493, `filetime`
  v0.2.29 #480)
- Routine wrangler updates (v4.90.1 → v4.97.0, #471 #478 #479 #485 #487 #488 #496)
- Routine tsx updates (v4.22.0 → v4.22.4, #477 #482 #483 #484 #492)
- Update `@types/bun` to v1.3.14 (#473)

## [0.2.1] - 2026-05-10

### Fixed

- **SDK credentials**: persist rotated `refresh_token` from OIDC refresh
  responses across all three SDKs (Rust, Go, TypeScript). Previously, a
  rotated token returned by the IdP was discarded, causing subsequent
  refreshes to fail once the original refresh token expired.

### Security

- **SDK credentials**: enforce `0o600` mode on `credentials.json` writes
  (refresh and login flows) so the file is readable only by its owner.

### Dependencies

- Bump `@bufbuild/protobuf` to 2.12.0 (#464)
- Update Terraform `terraform-aws-modules/key-pair/aws` to v3 (#459)
- Update Terraform `terraform-aws-modules/vpc/aws` to v6.6.1 (#439)
- Update Terraform `aws` provider to v6.44.0 (#445)
- Update GitHub Action `softprops/action-gh-release` to v3 (#449)
- Routine Cargo lockfile maintenance (#427)

## [0.1.0] - 2026-04-08

### Added

The first stable release of Vorpal — build and ship software with one
language-agnostic workflow.

- **Declarative builds** with Go, Rust and TypeScript SDKs
- **Cross-platform** support (aarch64/x86_64, macOS/Linux)
- **Reproducible** hermetic steps with pinned toolchains
- **Distributed architecture** — CLI, Agent, Registry, Worker
- **Flexible executors** — Bash, Bubblewrap, Docker, or custom

**Install**

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/refs/heads/main/script/install.sh | sh
```

**Documentation**

[Getting Started →](https://docs.vorpal.build)
