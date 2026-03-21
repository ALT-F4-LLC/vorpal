---
project: "vorpal"
maturity: "draft"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Code review strategy, quality gates, and PR workflow for the Vorpal project"
owner: "@staff-engineer"
dependencies:
  - code-quality.md
  - testing.md
---

# Review Strategy

## 1. Overview

This document describes the review processes, quality gates, and PR workflows that actually exist in the Vorpal project today. Vorpal is a multi-language build system (Rust CLI + core, with SDKs in Rust, Go, and TypeScript) maintained by a small team under the ALT-F4-LLC organization.

## 2. Current PR Workflow

### 2.1 Branching Model

- **Main branch**: `main` is the integration branch. All feature work merges here.
- **Feature branches**: Convention observed in commit history is direct pushes to `main` for small fixes and PR-based merges (evidenced by `(#NNN)` suffixes) for features and non-trivial changes.
- **Tag-based releases**: Releases are cut by pushing Git tags. Nightly releases use a `nightly` tag recreated daily against `main` HEAD via the `vorpal-nightly` workflow.

### 2.2 CI as the Primary Quality Gate

The project has **no CODEOWNERS file**, **no PR template**, **no CONTRIBUTING.md**, and **no documented review checklist**. The CI pipeline (`vorpal.yaml`) is the de facto and only enforced quality gate. The pipeline stages are:

| Stage | What It Checks | Blocking? |
|-------|---------------|-----------|
| `vendor` | Cargo dependency resolution and `cargo check` across 4 platform targets (macOS x86/ARM, Linux x86/ARM) | Yes |
| `code-quality` | `cargo fmt --all --check` (formatting) and `cargo clippy -- --deny warnings` (linting) | Yes |
| `build` | `cargo build --release`, dynamic dependency verification (no non-system dylibs), `cargo test --release`, dist tarball creation | Yes |
| `test` | Integration tests: builds Vorpal artifacts using the built binary, then cross-validates Rust, Go, and TypeScript SDK output parity | Yes |

All stages are sequential (`vendor` -> `code-quality` -> `build` -> `test`) and must pass before merge. Concurrency control (`cancel-in-progress: true`) ensures only the latest push per PR is tested.

### 2.3 What CI Does NOT Check

- **No Go linting or formatting** (e.g., `golangci-lint`, `gofmt`) -- Go SDK code is not statically analyzed in CI.
- **No TypeScript linting or formatting** (e.g., ESLint, Prettier) -- TypeScript SDK code is not statically analyzed in CI.
- **No security scanning** -- no SAST, dependency audit (`cargo audit`, `npm audit`), or secret scanning in the pipeline.
- **No coverage reporting** -- test coverage is not measured or enforced.
- **No documentation build verification** -- no docs site exists yet (planned per `docs/tdd/docs-framework-selection.md`).

### 2.4 Branch Protection

Branch protection rules could not be verified via API (TLS error in this session). Based on observed commit history:

- Some commits merge directly to `main` without PR numbers (e.g., `fix(github): ...` series without `(#NNN)`), suggesting branch protection may be relaxed or admin-bypassed for certain contributors.
- Most feature work and dependency updates arrive via PRs with squash merges.

## 3. Automated Dependency Review

### 3.1 Renovate Bot

Renovate is configured (`.github/renovate.json`) with a sophisticated automerge policy:

**Automerged without human review:**
- GitHub Actions: minor and patch updates
- Dev dependencies (all ecosystems): patch updates; minor updates for stable (>= 1.0) packages
- Rust production crates: patch and minor (stable only) with 3-day minimum release age
- Go modules: patch and minor (stable only) with 3-day minimum release age
- TypeScript production deps: patch and minor (stable only) with 3-day minimum release age
- Docker images: patch and minor (stable only) with 3-day minimum release age

**Require human review:**
- All major version bumps across all ecosystems
- Go indirect dependency updates (explicitly `automerge: false`)
- Terraform provider updates (explicitly `automerge: false`)
- Pre-1.0 minor updates for production dependencies

A companion workflow (`.github/workflows/renovate.yaml`) auto-approves Renovate PRs, enabling `platformAutomerge` to merge them once CI passes.

### 3.2 Lock File Maintenance

Renovate runs weekly lock file maintenance with automerge enabled, keeping transitive dependencies current without manual intervention.

## 4. Review Dimensions by Risk Area

This section maps the project's high-risk areas to the review dimensions that matter most for each.

### 4.1 High-Risk: gRPC/Protobuf API Surface

- **Location**: `sdk/rust/api/` (5 proto files: agent, archive, artifact, context, worker)
- **Impact**: API changes propagate to all three SDKs (Rust, Go, TypeScript) via `make generate`
- **Review focus**: Backward compatibility, wire format stability, cross-SDK parity
- **Current gap**: No automated proto breaking-change detection (e.g., `buf breaking`)

### 4.2 High-Risk: Cross-SDK Parity

- **Mechanism**: CI integration tests build the same artifacts (vorpal, vorpal-container-image, vorpal-job, vorpal-process, vorpal-shell, vorpal-user) with Rust, Go, and TypeScript SDKs and verify identical output hashes
- **Review focus**: Any SDK change must not break parity. The CI test is the enforcement mechanism.
- **Current strength**: This is well-automated and catches SDK divergence.

### 4.3 High-Risk: Release Pipeline

- **Mechanism**: Tag push triggers release jobs: binary artifacts (with build provenance attestation), Docker multi-arch images, crates.io publish, npm publish (with OIDC provenance)
- **Review focus**: Changes to workflow files, release scripts, or version bumps. Idempotency guards (version-exists checks) are in place for both crates.io and npm.
- **Current gap**: No staging/dry-run release path.

### 4.4 Medium-Risk: Build System / Nix-like Artifact Pipeline

- **Location**: `sdk/rust/src/artifact/` (core artifact types: clippy, language toolchains, etc.)
- **Review focus**: Determinism, sandbox escapes, static linking correctness (dynamic dep verification exists in CI)
- **Current strength**: CI verifies no non-system dynamic library dependencies on both macOS and Linux.

### 4.5 Medium-Risk: CLI Commands and Config

- **Location**: `cli/src/`
- **Review focus**: UX changes, flag compatibility, error messages
- **Current gap**: No CLI snapshot or golden-file tests.

### 4.6 Lower-Risk: Infrastructure (Terraform)

- **Location**: `terraform/`
- **Review focus**: State changes, cost impact, IAM changes
- **Current gap**: No `terraform plan` in CI. Terraform provider updates explicitly require human review (Renovate config).

## 5. Commit Convention

The project follows **Conventional Commits** as observed across 100+ recent commits:

- `feat(scope):` -- new features
- `fix(scope):` -- bug fixes
- `chore(deps):` -- dependency updates (automated by Renovate)
- `chore(scope):` -- maintenance tasks
- `docs(scope):` -- documentation changes

Common scopes: `deps`, `github`, `cli`, `sdk`, `install`, `ci`, `renovate`.

This convention is **not enforced by CI** -- there is no commitlint or conventional-commit check in the pipeline. Adherence is by team discipline.

## 6. Gaps and Recommendations

| Gap | Impact | Recommended Priority |
|-----|--------|---------------------|
| No CODEOWNERS file | No automatic reviewer assignment; relies on team awareness | High -- easy to add, immediate value for a growing team |
| No PR template | Review quality varies; easy to forget cross-SDK impact checks | High -- template should prompt for SDK parity, proto compat, and test evidence |
| No CONTRIBUTING.md | Barrier to external contributors; implicit conventions only | Medium -- important for open-source adoption |
| No Go/TypeScript linting in CI | Code quality enforcement is Rust-only | Medium -- Go and TS SDKs ship to users via crates.io/npm |
| No security scanning in CI | Vulnerabilities in dependencies not caught until manual audit | Medium -- `cargo audit` and `npm audit` are low-effort additions |
| No branch protection verification | Direct pushes to `main` may bypass CI | Medium -- should enforce PR-only merges with required status checks |
| No proto breaking-change detection | API breakage discovered only at build/test time | Low-Medium -- `buf breaking` or similar would shift left |
| No conventional commit enforcement | Convention drift possible as team grows | Low -- current discipline is strong |
| No coverage reporting | Cannot track test coverage trends or set thresholds | Low -- minimal unit tests exist currently; integration tests are the primary safety net |

## 7. Review Workflow Summary

```
Developer pushes branch
         |
         v
    CI runs automatically
    (vendor -> code-quality -> build -> test)
         |
    +----+----+
    |         |
  PASS      FAIL --> fix and push again
    |
    v
  PR opened (no template enforced)
    |
    v
  Manual review (no CODEOWNERS assignment)
    |
    v
  Squash merge to main
    |
    v
  Tag push (manual) --> release pipeline
```

For Renovate dependency PRs:
```
Renovate opens PR --> CI runs --> auto-approve workflow --> platformAutomerge
(if automerge policy allows; otherwise awaits human review)
```
