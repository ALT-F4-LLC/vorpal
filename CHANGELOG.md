# Changelog

All notable changes to this project will be documented in this file.

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
