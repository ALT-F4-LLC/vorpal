---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Code review strategy, quality gates, and change approval processes for the Vorpal project"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
  - testing.md
  - security.md
---

# Review Strategy

## 1. Overview

This document describes the code review strategy for Vorpal — a cross-platform build system with
a Rust core, gRPC API surface, and multi-language SDKs (Rust, Go, TypeScript). It covers existing
automated quality gates, areas requiring elevated review scrutiny, and gaps in the current review
process.

## 2. Current State of Automated Quality Gates

### 2.1 CI Pipeline (GitHub Actions)

The primary CI workflow (`.github/workflows/vorpal.yaml`) enforces a sequential gate model on
every PR and push to `main`:

| Stage | Gate | What It Checks |
|-------|------|----------------|
| `vendor` | `cargo check --release` | Compilation against vendored dependencies on all 4 platform matrix targets (macOS x86/ARM, Linux x86/ARM) |
| `code-quality` | `cargo fmt --all --check` | Formatting conformance (rustfmt) |
| `code-quality` | `cargo clippy --release -- --deny warnings` | Lint-level correctness — clippy warnings are treated as errors |
| `build` | `cargo build --release` | Full release build on all 4 platform matrix targets |
| `build` | Dynamic dependency verification | `otool -L` (macOS) / `ldd` (Linux) asserts no non-system dynamic library linkage (liblzma, libzstd, liblz4, libbrotli, homebrew/local libs) |
| `build` | `cargo test --release` | Unit test suite |
| `build` | `make dist` | Distribution tarball creation |
| `test` | Cross-SDK artifact parity | Builds the same artifacts via Rust, Go, and TypeScript SDK configs and asserts digest equality — ensures SDK behavioral consistency |
| `release` | Tag-gated release | Only runs on tag pushes — creates GitHub release with provenance attestation |
| `release` | SDK publishing | Publishes to crates.io (Rust SDK) and npm (TypeScript SDK) with version-existence guards |

**Key observations:**

- All CI jobs run inside a `./script/dev.sh` wrapper, which provides a consistent development
  environment.
- Caching is used aggressively (`actions/cache`) for `target/` and `vendor/` directories, keyed
  by architecture, OS, and `Cargo.lock` hash.
- The pipeline enforces **static linking** — the dynamic dependency verification step is a custom
  gate that prevents accidental linkage to non-system libraries. This is critical for Vorpal's
  cross-platform binary distribution model.
- Build provenance attestation (`actions/attest-build-provenance`) is generated for release
  binaries.

### 2.2 Dependency Management (Renovate)

Renovate is configured (`.github/renovate.json`) with a tiered automerge strategy:

| Category | Automerge Policy |
|----------|-----------------|
| GitHub Actions minor/patch | Automerge |
| Dev dependencies patch | Automerge |
| Dev dependencies minor (>= 1.0) | Automerge |
| Prod Rust/Go/TypeScript/Docker patches | Automerge after 3-day soak |
| Prod Rust/Go/TypeScript/Docker minor (>= 1.0) | Automerge after 3-day soak |
| Go indirect dependencies | Manual review required |
| Terraform providers | Manual review required |
| Vorpal SDK in Go template | Ignored (managed separately) |

A dedicated workflow (`.github/workflows/renovate.yaml`) auto-approves Renovate bot PRs. Lock
file maintenance runs weekly with automerge.

**Risk note:** The combination of auto-approve + automerge for Renovate PRs means dependency
updates bypass human review entirely for qualifying categories. The 3-day `minimumReleaseAge`
for production dependencies provides some supply-chain protection, but there is no additional
verification (e.g., no license audit, no SBOM generation, no vulnerability scanning step in CI).

### 2.3 Pre-Commit / Local Checks

There are **no** local pre-commit hooks, commit-msg hooks, or `.editorconfig` files. Developers
rely on the CI pipeline as the sole automated quality gate. The `makefile` provides local
equivalents (`make format`, `make lint`, `make check`, `make test`) but these are not enforced
before push.

## 3. Review Dimensions by Risk Area

### 3.1 High-Risk Areas (Require Thorough Review)

These areas carry the highest blast radius and should receive structured review across all
dimensions (architecture, security, operations, performance, correctness):

#### gRPC API Contracts (`sdk/rust/api/**/*.proto`)

- **Why:** Proto files define the contract between the CLI, agent, worker, registry, and all SDK
  consumers. Changes here cascade to Go and TypeScript generated code.
- **Review focus:** Backward compatibility (field numbering, message evolution), semantic
  correctness of new fields, impact on cross-SDK parity test.
- **Current gap:** No proto linting (e.g., `buf lint`) or breaking-change detection (e.g.,
  `buf breaking`) in CI.

#### Agent / Worker Build Execution (`cli/src/command/start/agent.rs`, `cli/src/command/start/worker.rs`)

- **Why:** These modules execute build steps, handle artifact sources (local, git, HTTP), manage
  sandboxing, and interact with the registry. They are the most frequently changed Rust files
  and handle untrusted input (artifact definitions, source URLs).
- **Review focus:** Input validation, sandbox escape vectors, error handling around network/IO
  operations, correct artifact digest computation.

#### Authentication & Authorization (`cli/src/command/start/auth.rs`)

- **Why:** Implements OIDC JWT validation with JWK rotation, namespace-scoped permissions. A
  bypass here grants unauthorized registry access.
- **Review focus:** Token validation completeness (expiry, audience, issuer), JWK caching
  correctness, namespace permission enforcement.

#### Registry Storage Backends (`cli/src/command/start/registry/`)

- **Why:** Manages artifact and archive storage across local and S3 backends. Data integrity and
  availability depend on correct implementation.
- **Review focus:** Data consistency (digest verification on read/write), error handling for
  partial uploads, S3 credential handling.

#### CLI Entry Point / Command Routing (`cli/src/command.rs` — ~600 lines, most-changed file)

- **Why:** Central command dispatch with complex argument handling. Highest change frequency
  in the codebase.
- **Review focus:** Correct subcommand routing, flag handling edge cases, backward compatibility
  of CLI interface.

### 3.2 Medium-Risk Areas

#### SDK Libraries (`sdk/rust/src/`, `sdk/go/`, `sdk/typescript/`)

- **Review focus:** API surface consistency across languages, correct gRPC client construction,
  artifact helper behavior parity.
- **Special concern:** The Go and TypeScript SDKs are published packages (`vorpal-sdk` on
  crates.io, `@altf4llc/vorpal-sdk` on npm). Changes to their public API surface constitute
  breaking changes for external consumers.

#### Config / Self-Build System (`config/src/`)

- **Review focus:** The config crate builds Vorpal itself (bootstrap). Changes here can break
  the self-build cycle. Artifact definitions for containers, jobs, processes, shells, and users
  must produce deterministic outputs.

#### Terraform Infrastructure (`terraform/`)

- **Review focus:** Infrastructure changes affect production registry and services. Terraform
  provider updates are correctly excluded from Renovate automerge.

### 3.3 Lower-Risk Areas

- **Templates** (`cli/src/command/template/`): Project scaffolding. Review for correctness and
  version pinning.
- **Scripts** (`script/`): Development tooling. Review for portability and idempotency.
- **Documentation**: Review for accuracy against current codebase state.

## 4. What Exists vs. What Is Missing

### 4.1 Exists

| Artifact | Status |
|----------|--------|
| CI pipeline with format, lint, build, test gates | Active, enforced on all PRs |
| Cross-platform build matrix (4 targets) | Active |
| Cross-SDK parity testing | Active — digest comparison between Rust, Go, TypeScript builds |
| Static linking verification | Active — custom gate in CI |
| Renovate dependency management | Active with tiered automerge policy |
| Build provenance attestation | Active on releases |
| Nightly release workflow | Active (daily cron) |

### 4.2 Missing

| Artifact | Impact |
|----------|--------|
| PR template | No structured checklist for reviewers — review quality depends on individual discipline |
| CODEOWNERS file | No automatic reviewer assignment — risk of changes landing without domain-expert review |
| CONTRIBUTING guide | No documented review expectations for external contributors |
| Branch protection rules | Could not verify — but no evidence of required reviewers, status checks, or signed commits in repo config |
| Proto linting / breaking-change detection | API contract changes could break backward compatibility without warning |
| Security scanning (SAST/dependency audit) | No `cargo audit`, `npm audit`, or similar in CI |
| License compliance checking | No license audit for dependencies |
| Integration test suite | Only 1 Rust test module (`cli/src/command/start/registry.rs`) and 2 Go test files; no integration tests in CI beyond the cross-SDK parity check |
| Code coverage tracking | No coverage measurement or reporting |
| Performance benchmarks in CI | No regression detection for build performance |
| Pre-commit hooks | No local enforcement of format/lint before push |

## 5. Recommended Review Workflow

### 5.1 Change Classification

| Change Type | Review Requirement |
|-------------|-------------------|
| Proto file changes | Block until backward-compatibility is verified; require cross-SDK parity test pass |
| Auth/security changes | Require security-focused review; consider independent validation |
| Agent/worker build execution | Require review by domain expert; check for sandbox and input validation concerns |
| SDK public API changes | Require semver assessment; document breaking changes |
| Terraform changes | Require infrastructure review; plan output should be included in PR |
| Dependency updates (manual) | Verify changelog, check for known vulnerabilities, assess transitive impact |
| CI workflow changes | Test in a branch first; review for secret exposure |
| Template/script/docs | Standard review sufficient |

### 5.2 Review Checklist (Recommended for Adoption)

For non-trivial changes, reviewers should assess:

1. **Intent alignment** — Does the change solve the stated problem? Is the scope appropriate?
2. **Backward compatibility** — Will this break existing users, SDK consumers, or API clients?
3. **Cross-platform correctness** — Does this work on all 4 target platforms? Are platform-specific
   code paths handled?
4. **Error handling** — Are failures handled gracefully? Are error messages actionable?
5. **Security** — Are inputs validated? Are secrets handled correctly? Any new attack surface?
6. **Observability** — Are operations logged at appropriate levels? Can failures be diagnosed
   from logs alone?
7. **Test coverage** — Are new code paths tested? Are edge cases covered?
8. **SDK parity** — If proto or SDK behavior changes, are all three language SDKs updated?

### 5.3 Approval Model

**Current state:** No formal approval model is documented or enforced via tooling. The project
has a small contributor base (2 primary contributors + Renovate bot), which has allowed informal
review to work at current scale.

**Recommendation for growth:** As the project matures, establish:

- Required reviewer count (minimum 1 human approval for non-Renovate PRs)
- CODEOWNERS mapping for high-risk areas (proto files, auth, agent/worker, SDKs)
- Branch protection requiring status checks to pass before merge

## 6. Release Review Gates

The release process is tag-triggered and has the following existing gates:

1. All CI stages must pass (vendor, code-quality, build, test)
2. Cross-SDK parity verified
3. Version existence check prevents duplicate publishes to crates.io and npm
4. Build provenance attestation generated for release binaries
5. Docker multi-arch manifest created for container releases

**Missing release gates:**

- No changelog generation or release notes beyond tag name
- No pre-release validation beyond CI (no staging environment, no canary deployment)
- No rollback procedure documented
- Nightly releases delete and recreate the `nightly` tag — no retention of previous nightlies

## 7. Dependency Review Strategy

### 7.1 Existing Controls

- Renovate with tiered automerge and 3-day soak for production dependencies
- Vendored Rust dependencies (`cargo vendor`) — provides reproducible builds and offline
  compilation
- Pinned Rust toolchain (`rust-toolchain.toml`: channel 1.93.1)
- Renovate excludes Vorpal SDK updates in Go template directory (prevents circular updates)

### 7.2 Gaps

- No `cargo audit` or equivalent vulnerability scanning in CI
- No SBOM (Software Bill of Materials) generation
- No license compliance verification
- Go indirect dependency updates require manual review (good) but lack structured criteria
- Terraform provider updates require manual review (good) but lack plan-output requirement
