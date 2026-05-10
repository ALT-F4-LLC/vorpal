# Changelog

All notable changes to this project will be documented in this file.

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
