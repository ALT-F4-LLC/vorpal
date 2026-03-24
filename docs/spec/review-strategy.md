---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Code review strategy, quality gates, and change management practices for the vorpal project"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
  - testing.md
---

# Review Strategy

## 1. Overview

Vorpal is a single-maintainer open-source project (Erik Reinert, ~179 of ~365 human commits) with
occasional external contributions. The review strategy reflects this reality: automated quality
gates carry most of the burden, with human review reserved for external contributions and
significant architectural changes.

## 2. Current Review Infrastructure

### 2.1 CI Quality Gates (GitHub Actions)

The primary CI workflow (`.github/workflows/vorpal.yaml`) enforces a sequential gate pipeline on
every pull request and push to `main`:

| Stage | Job | What It Checks |
|-------|-----|----------------|
| 1 | `vendor` | `cargo check --offline --release` across 4 platform matrix (macOS x86_64, macOS aarch64, Ubuntu x86_64, Ubuntu aarch64) |
| 2 | `code-quality` | `cargo fmt --all --check` (formatting) and `cargo clippy --release -- --deny warnings` (linting) |
| 3 | `build` | `cargo build --release`, dynamic dependency verification (no non-system libs linked), `cargo test --release`, distribution packaging |
| 4 | `test` | End-to-end integration tests using built binary -- builds vorpal artifacts via Rust, Go, and TypeScript SDKs and cross-validates output hashes |

Jobs are sequential: `vendor` -> `code-quality` -> `build` -> `test`. A failure at any stage blocks
all downstream stages.

**Key properties:**
- Concurrency control: `cancel-in-progress: true` per PR/branch -- superseded runs are cancelled
- Cross-platform: all stages run on macOS (x86_64, aarch64) and Ubuntu (x86_64, aarch64)
- Code quality runs on single platform (macOS) since formatting/linting is platform-independent
- Dynamic linking verification prevents accidental non-system library dependencies in release builds

### 2.2 Automated Dependency Management (Renovate)

Renovate (`.github/renovate.json`) manages dependency updates with a tiered automerge policy:

| Category | Automerge Policy | Minimum Release Age |
|----------|------------------|---------------------|
| GitHub Actions (minor/patch) | Auto | None |
| Dev dependencies (patch) | Auto | None |
| Dev dependencies (minor, >= 1.0) | Auto | None |
| Production deps (patch, all ecosystems) | Auto | 3 days |
| Production deps (minor, >= 1.0, all ecosystems) | Auto | 3 days |
| Go indirect dependencies | **Manual** | N/A |
| Terraform providers | **Manual** | N/A |
| Major version bumps (all) | **Manual** | N/A |
| Pre-1.0 minor bumps (production) | **Manual** | N/A |

A companion workflow (`.github/workflows/renovate.yaml`) auto-approves Renovate PRs, allowing
`platformAutomerge` to merge them once CI passes.

**Notable:** Renovate accounts for roughly half of all commits (~176 of ~365), indicating heavy
reliance on automated dependency maintenance.

### 2.3 Pre-commit Hooks

None. There are no `.pre-commit-config.yaml`, git hooks, or local pre-commit enforcement. All
quality checks run exclusively in CI.

### 2.4 PR Templates and Contribution Guidelines

None. There is no `PULL_REQUEST_TEMPLATE.md`, `CONTRIBUTING.md`, or `CODEOWNERS` file. External
contributors have no documented guidance on PR expectations.

## 3. Review Dimensions and Risk Areas

### 3.1 High-Risk Areas Requiring Careful Review

1. **gRPC API contracts** (`sdk/rust/api/*.proto`) -- Proto definitions are the cross-SDK contract.
   Changes here cascade to Rust, Go, and TypeScript SDKs. The `makefile generate` target regenerates
   Go and TypeScript bindings from these protos.

2. **Artifact build logic** (`sdk/rust/src/artifact/`, `sdk/rust/src/context.rs`) -- Core build
   system logic. The SDK is published to crates.io; breaking changes affect downstream consumers.

3. **Security boundaries** -- TLS certificate generation (`rcgen`, `rustls`), JWT handling
   (`jsonwebtoken`), OAuth2 flows (`oauth2`), RSA key operations (`rsa`). No `unsafe` code exists,
   which is good, but cryptographic configuration changes need scrutiny.

4. **Platform-specific sandboxing** -- The project uses `bwrap` (bubblewrap) for Linux sandboxing.
   Environment variable handling in sandbox steps was a recent bug fix (`0cf0d9b`).

5. **Release pipeline** (`.github/workflows/vorpal.yaml` release jobs) -- Publishes binaries to
   GitHub Releases, container images to Docker Hub, Rust SDK to crates.io, and TypeScript SDK to
   npm. Build provenance attestation is enabled for binaries. Changes here have irreversible
   consequences (published packages cannot be unpublished easily).

### 3.2 Review Dimensions by Priority

Given the project's nature as an infrastructure/build tool:

1. **Correctness** -- Build reproducibility is a core value (cross-SDK hash validation in CI proves
   Rust, Go, and TypeScript SDKs produce identical artifacts).
2. **Security** -- The tool manages TLS, OIDC tokens, and runs sandboxed processes.
3. **Cross-platform compatibility** -- Must work on macOS and Linux, x86_64 and aarch64.
4. **API stability** -- Published SDKs (crates.io, npm) mean breaking changes affect external users.
5. **Operational safety** -- Dynamic dependency checks prevent shipping binaries that depend on
   host-specific libraries.

## 4. Change Management Process

### 4.1 Current Workflow

Based on commit history and CI configuration:

1. **Main branch**: Direct pushes and PR merges both observed. No branch protection rules are
   verifiable from the repository contents alone.
2. **Tagging for releases**: Tag pushes (any `*` pattern) trigger the release pipeline. Nightly
   releases are automated via cron schedule (`.github/workflows/vorpal-nightly.yaml`, daily at
   08:00 UTC).
3. **Commit convention**: Conventional commits are used (`feat:`, `fix:`, `chore:`, `docs:`).
   Renovate is configured with `:semanticCommitTypeAll(chore)`.

### 4.2 Release Gates

Releases require all 4 CI stages to pass (`vendor` -> `code-quality` -> `build` -> `test`).
Additional release-specific checks:

- **SDK version deduplication**: Both `release-sdk-rust` and `release-sdk-typescript` jobs check
  whether the version already exists on crates.io/npm before publishing, preventing duplicate
  publish attempts.
- **Build provenance**: Binary releases include `actions/attest-build-provenance@v4` attestation.
  The TypeScript SDK uses `npm publish --provenance`.
- **Container images**: Multi-arch manifest (amd64 + arm64) published to Docker Hub.

### 4.3 Nightly Releases

A separate workflow creates nightly tag releases by:
1. Deleting any existing `nightly` release and tag
2. Creating a new `nightly` tag pointing to the current `main` HEAD
3. This triggers the main workflow's release pipeline

## 5. Gaps and Recommendations

### 5.1 Missing Artifacts

| Missing | Impact | Priority |
|---------|--------|----------|
| `CODEOWNERS` file | No automatic reviewer assignment for PRs | Medium |
| `CONTRIBUTING.md` | External contributors lack guidance on PR expectations, commit conventions, and review process | Medium |
| PR template | No structured checklist for PR authors | Low |
| Pre-commit hooks | Formatting and lint issues only caught in CI (slower feedback) | Low |
| Branch protection (unverified) | Cannot confirm whether force-push to `main` is blocked | High |

### 5.2 Coverage Gaps in CI

- **No security scanning**: No `cargo audit`, `cargo deny`, Dependabot alerts, or SAST tooling in
  CI. Dependency updates are managed by Renovate but without vulnerability-specific scanning.
- **No Go/TypeScript linting**: CI only runs Rust formatting and clippy. The Go and TypeScript SDKs
  have no dedicated lint or format checks.
- **No proto breaking-change detection**: No `buf` or equivalent tool to catch breaking proto
  changes before they land.
- **Single-platform code quality**: Format and lint checks run only on macOS. While platform
  differences in formatting are unlikely, this means any platform-conditional Rust code is only
  linted on one platform.

### 5.3 Test Coverage Visibility

- No coverage reporting tool (e.g., `cargo-tllvm-cov`, `tarpaulin`) is configured.
- Unit tests are minimal (only 1 `#[test]` attribute found across the entire Rust codebase). The
  project relies heavily on end-to-end integration tests (cross-SDK artifact hash validation) rather
  than unit tests.

### 5.4 Review Process Formalization

For a single-maintainer project, the current approach (automated gates + self-merge) is pragmatic.
If the contributor base grows, the following should be formalized:

- Required reviewers for changes touching proto files, release workflows, and security-related crates
- Separate review requirements for SDK-published code vs. internal CLI code
- Documented decision on whether TDD approval (via `/vote` consensus) is required before
  implementation begins for significant changes
