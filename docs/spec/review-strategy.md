---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "Code review priorities, risk areas, and review workflow"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - code-quality.md
  - testing.md
---

# Review Strategy

## Overview

Vorpal is a build system with three SDK implementations that must maintain digest parity, gRPC service boundaries defined by protobuf contracts, and content-addressed storage that demands correctness. This spec identifies the high-risk areas and outlines review priorities.

## High-Risk Areas

### 1. Protobuf API Changes (`sdk/rust/api/`)

**Risk: Breaking cross-SDK compatibility**

Any change to `.proto` files affects all three SDKs (Rust, Go, TypeScript). Proto changes require:
- Regeneration of Go stubs (`make generate`)
- Regeneration of TypeScript stubs (`make generate`)
- Verification that all three SDKs still produce identical artifact digests
- No backwards-incompatible field removals or renumbering

**Review checklist:**
- [ ] Field numbers are not reused or changed
- [ ] New fields use `optional` where appropriate
- [ ] `make generate` has been run and generated code is committed
- [ ] CI cross-SDK parity tests pass

### 2. Content-Addressed Digest Computation

**Risk: Cache invalidation or false cache hits**

Changes to how digests are computed (source hashing, artifact serialization, file timestamp normalization) can silently break caching or cause incorrect builds.

**Critical files:**
- `cli/src/command/store/hashes.rs` -- Source digest computation
- `cli/src/command/start/agent.rs` -- Artifact digest computation (SHA-256 of JSON-serialized artifact)
- `cli/src/command/store/paths.rs` -- File timestamp normalization

**Review checklist:**
- [ ] Digest computation is deterministic across platforms
- [ ] File ordering is sorted before hashing
- [ ] Timestamp normalization is applied consistently
- [ ] Changes don't invalidate existing caches without intentional migration

### 3. Authentication and Authorization (`cli/src/command/start/auth.rs`)

**Risk: Security bypass or credential exposure**

**Review checklist:**
- [ ] JWT validation enforces issuer, audience, expiry, and not-before
- [ ] Namespace permission checks are applied to all registry mutation endpoints
- [ ] No credentials logged at INFO level or below
- [ ] Token refresh handles edge cases (expired JWKS, network failures)

### 4. Source Resolution (`cli/src/command/start/agent.rs`)

**Risk: Supply chain attacks via source substitution, data corruption**

The agent downloads HTTP sources, decompresses various formats, and computes digests. This is the primary attack surface.

**Review checklist:**
- [ ] Downloaded content verified against expected digest
- [ ] Archive extraction is sandboxed to target directory (no path traversal)
- [ ] Compression format detection uses content sniffing (not extension)
- [ ] Lock file enforcement prevents unexpected source changes (without `--unlock`)

### 5. Build Step Execution (Worker)

**Risk: Sandbox escape, unintended host access**

**Review checklist:**
- [ ] Environment variables don't leak sensitive data
- [ ] Secrets are encrypted before transit
- [ ] Executor sandboxing is properly configured
- [ ] Output paths are validated

## Areas of Frequent Change

Based on recent commit history and codebase structure:

1. **Dependency updates** -- Renovate generates frequent PRs. Most are auto-merged per policy, but major version bumps and pre-1.0 crate updates require manual review.

2. **SDK artifact builders** (`sdk/rust/src/artifact/`) -- New language support, tool additions, and OS image updates. High file count (30+ artifact builders in Rust SDK alone).

3. **CLI command handling** (`cli/src/command/`) -- New subcommands, flag additions, configuration layering. This is where most feature work lands.

4. **CI workflow changes** (`.github/workflows/`) -- Runner updates, new release targets, cache key changes.

## Review Dimensions by Priority

### P0 -- Always Review Carefully

- **Correctness**: Digest computation, protobuf contracts, cross-SDK parity
- **Security**: Auth/authz changes, secret handling, source resolution
- **Data integrity**: Store operations, lockfile updates, archive compression

### P1 -- Review for Regressions

- **Cross-platform**: Changes that affect macOS vs Linux behavior, architecture-specific code
- **Configuration**: Layered config resolution, default value changes, CLI flag semantics
- **Error handling**: New error paths, status code mapping, user-facing error messages

### P2 -- Verify Standards

- **Code style**: Rust formatting, naming conventions, module organization
- **Documentation**: Public API docs, CLI help text, README accuracy
- **Dependencies**: Version pinning, feature flags, unused dependency detection

## Existing Review Infrastructure

### CI Quality Gates

The CI pipeline enforces these gates before merge:

1. `cargo fmt --all --check` -- No formatting violations
2. `cargo clippy -- --deny warnings` -- No clippy warnings
3. `cargo test` -- All unit tests pass
4. `cargo build` (release mode) -- Clean build on all 4 platforms
5. Dynamic library check -- No non-system library dependencies
6. Cross-SDK parity -- Go and TypeScript digests match Rust

### What CI Does NOT Check

- No integration test coverage reporting
- No Go or TypeScript linting
- No dependency vulnerability scanning
- No PR template or review checklist enforcement
- No CODEOWNERS file for automatic reviewer assignment
- No required review count configuration visible in the repo

## Contribution Guidelines

No `CONTRIBUTING.md` file exists. The README provides minimal guidance:

```bash
# Build from source
./script/dev.sh make build

# Before submitting a PR
make format && make lint && make test
```

## Gaps and Recommendations

- No `CODEOWNERS` file -- critical paths (auth, digest computation, protos) should have designated reviewers
- No PR template -- would help ensure reviewers check cross-SDK parity and proto regeneration
- No `CONTRIBUTING.md` -- needed for external contributors
- No automated review assignment
- No required review count enforcement visible in the repo configuration
- Renovate auto-merge policies are well-configured but there's no security advisory integration (e.g., `cargo audit` in CI)
- No changelog automation
