# Review Strategy

How code review works for Vorpal, which areas carry the most risk, and what reviewers should
focus on for each type of change.

---

## 1. Project Review Profile

Vorpal is a multi-language build system with a Rust CLI/server core, three SDK implementations
(Rust, Go, TypeScript), gRPC APIs defined in protobuf, and a CI pipeline that enforces
cross-SDK artifact parity. This combination creates several review dynamics:

- **Cross-language consistency is critical.** Changes to one SDK often require identical
  behavior in all three. Reviewers must check whether a change to `sdk/rust/` needs
  corresponding changes in `sdk/go/` and `sdk/typescript/`.
- **Proto-first API surface.** The protobuf definitions in `sdk/rust/api/` are the source of
  truth. Changes to `.proto` files cascade into generated Go and TypeScript code and must be
  reviewed for backward compatibility.
- **Content-addressed artifact integrity.** The system relies on SHA-256 digests for
  correctness. Any change to artifact serialization, hashing, or source handling can silently
  break reproducibility — and CI's SDK parity tests are the primary safety net.
- **Relatively small team with high blast radius.** ~303 total commits, no CONTRIBUTING.md,
  no PR template, no formal review checklist. Changes merge directly after CI passes. This
  makes careful human review more important, not less.

---

## 2. High-Risk Areas

These areas of the codebase carry disproportionate risk and warrant thorough review.

### 2.1 Worker Build Pipeline (`cli/src/command/start/worker.rs` — 1002 lines)

The largest single file in the project. Handles:
- Artifact building, source pulling, and archive push/pull
- Sandbox command execution via `tokio::process::Command`
- Environment variable expansion with custom `expand_env` function
- Lock file management for build coordination
- Service-to-service OAuth2 authentication

**Review focus:** Command injection via environment expansion, lock file race conditions,
incomplete cleanup on failure paths, correct handling of gRPC streaming errors.

### 2.2 SDK Context / Config (`sdk/rust/src/context.rs` — 776 lines)

Core orchestration layer that:
- Manages the artifact store (in-memory HashMap)
- Handles TLS channel construction (including Unix domain sockets)
- Implements OAuth2 token refresh with credential file I/O
- Runs a gRPC ContextService server
- Parses artifact aliases with validation

**Review focus:** Token refresh race conditions, credential file write safety, artifact
deduplication correctness (input vs output digest caching), TLS configuration security.

### 2.3 Build Command (`cli/src/command/build.rs` — 736 lines)

Orchestrates the full build flow:
- Config language detection and builder dispatch (Go/Rust/TypeScript)
- Config binary compilation and execution as a subprocess
- Artifact dependency resolution and ordered builds
- Archive pull/unpack with streaming gRPC

**Review focus:** Subprocess execution safety, archive handling (zstd decompress +
filesystem writes), correct dependency ordering, error propagation vs. `exit(1)`.

### 2.4 Authentication System (`cli/src/command/start/auth.rs` — 431 lines)

OIDC-based auth with:
- JWT validation using JWKS with key rotation support
- Namespace-based permission model
- OAuth2 Client Credentials Flow for service-to-service auth
- Sync-in-async `block_in_place` pattern for gRPC interceptors

**Review focus:** JWT validation correctness (audience, issuer, expiry), JWKS refresh
behavior, permission boundary enforcement, the `block_in_place` workaround for potential
deadlocks.

### 2.5 Agent Service (`cli/src/command/start/agent.rs` — 711 lines)

Handles artifact preparation including:
- Source packaging and digest computation
- Lock file resolution and management
- Archive upload for sources
- Interaction with both registry and worker services

**Review focus:** Source digest correctness, data integrity during archive operations,
proper error handling for partial failures.

### 2.6 Protobuf API Definitions (`sdk/rust/api/**/*.proto`)

Five proto files define the entire API surface:
- `artifact.proto` — Artifact CRUD, alias resolution, system enum
- `agent.proto` — Artifact preparation
- `archive.proto` — Archive push/pull streaming
- `context.proto` — Config context service
- `worker.proto` — Build execution

**Review focus:** Backward compatibility of message/field changes, correct field numbering,
any additions to the `ArtifactSystem` enum (requires updates across all three SDKs).

### 2.7 Cross-SDK Parity

The CI pipeline (`vorpal.yaml` test job) builds the same artifacts with Rust, Go, and
TypeScript SDKs and compares digests. Any divergence fails the build. This makes SDK changes
especially sensitive:

- `sdk/rust/src/artifact/` — Rust artifact builders
- `sdk/go/pkg/artifact/` — Go artifact builders (must produce identical output)
- `sdk/typescript/src/artifact/` — TypeScript artifact builders (must produce identical output)

**Review focus:** Identical default versions for toolchains/packages across SDKs, identical
step construction logic, identical source handling. The `builder.go` (809 lines) and
`artifact.ts` (1072 lines) files are the Go and TypeScript equivalents of the Rust
`artifact.rs` (698 lines).

---

## 3. Review Dimensions by Priority

For this project, these dimensions are ordered by importance:

| Priority | Dimension | Why |
|----------|-----------|-----|
| 1 | **Correctness** | Content-addressed builds must be deterministic. Wrong hashes = broken system. |
| 2 | **Security** | Worker executes arbitrary commands in sandboxes; auth protects multi-tenant registries. |
| 3 | **Cross-SDK Consistency** | Unique to this project. Any SDK divergence breaks CI and user trust. |
| 4 | **API Compatibility** | Proto changes are hard to roll back; clients may be on different versions. |
| 5 | **Operations** | gRPC services run as long-lived daemons; lock files and temp dirs need cleanup. |
| 6 | **Code Quality** | Important but secondary to functional correctness. |

---

## 4. Review Strategy by Change Type

### 4.1 SDK Artifact Builder Changes

Changes to files in `sdk/rust/src/artifact/`, `sdk/go/pkg/artifact/`, or
`sdk/typescript/src/artifact/`.

**Checklist:**
- [ ] Change is replicated identically across all three SDKs (or a tracking issue exists)
- [ ] Default package/toolchain versions match across SDKs
- [ ] Step construction produces identical artifact JSON serialization
- [ ] Source includes/excludes are consistent
- [ ] CI parity tests pass (Rust vs Go vs TypeScript digest comparison)

### 4.2 Proto API Changes

Changes to `sdk/rust/api/**/*.proto`.

**Checklist:**
- [ ] Field numbers are not reused or changed
- [ ] New fields use `optional` or `repeated` (not required) for forward compatibility
- [ ] `make generate` has been run to update Go and TypeScript generated code
- [ ] All three SDKs compile against the updated protos
- [ ] No breaking changes to existing RPC signatures without a migration plan

### 4.3 CLI Service Changes (Agent, Worker, Registry)

Changes to `cli/src/command/start/`.

**Checklist:**
- [ ] Error paths clean up resources (temp dirs, lock files, archive files)
- [ ] gRPC streaming handlers handle disconnection/cancellation
- [ ] Auth checks are present on new or modified endpoints
- [ ] Subprocess commands don't pass unsanitized user input
- [ ] Lock file operations are safe against concurrent access

### 4.4 Auth/Security Changes

Changes to `cli/src/command/start/auth.rs` or credential-related code in `sdk/rust/src/context.rs`.

**Checklist:**
- [ ] JWT validation includes audience, issuer, and expiry checks
- [ ] Namespace permission checks are applied to all data-modifying operations
- [ ] Credentials are not logged (even at debug level)
- [ ] Token refresh handles network failures gracefully
- [ ] OIDC discovery validates the issuer field matches expectation

### 4.5 Dependency Updates

Renovate bot automatically opens PRs for dependency updates. These are usually low-risk but
need attention for:

**Checklist:**
- [ ] Rust dependencies: `cargo check` and `cargo clippy` pass
- [ ] Major version bumps have changelog reviewed for breaking changes
- [ ] SDK dependency versions (e.g., `tonic`, `prost`, `grpc-js`) stay compatible across SDKs
- [ ] Lock files are updated (Cargo.lock, go.sum, bun.lock)

### 4.6 Build/CI Changes

Changes to `makefile`, `.github/workflows/`, or `script/`.

**Checklist:**
- [ ] CI matrix still covers all 4 platforms (aarch64-darwin, aarch64-linux, x86_64-darwin, x86_64-linux)
- [ ] Cache keys are correct (based on `Cargo.lock` hash)
- [ ] Secret references are valid
- [ ] No accidental changes to release/tag workflows

---

## 5. Existing Automated Checks

### CI Pipeline (`.github/workflows/vorpal.yaml`)

The pipeline enforces a strict sequence:

1. **vendor** — `cargo check` on all 4 platforms (validates Rust compilation)
2. **code-quality** — `cargo fmt --check` + `cargo clippy -- --deny warnings`
3. **build** — `cargo build` + `cargo test` + binary distribution packaging
4. **test** — End-to-end SDK parity: builds artifacts with Rust SDK, then rebuilds with Go
   and TypeScript SDKs, comparing digests for exact match
5. **container-image** + **release** — Tag-triggered deployment (only on push to tag)

### What CI Catches
- Rust compilation errors, formatting violations, clippy warnings (treated as errors)
- SDK parity failures (digest mismatch between Rust/Go/TypeScript builds)
- Platform-specific build failures (4-platform matrix)

### What CI Does Not Catch
- Logic errors that produce consistent-but-wrong digests across all SDKs
- Security vulnerabilities in auth/permission logic
- Resource leaks (temp dirs, lock files, gRPC connections)
- Race conditions in concurrent builds
- Performance regressions
- Backward compatibility of proto/API changes (no integration test suite for this)

### Renovate (`.github/renovate.json`)

Automated dependency updates with:
- Weekly lock file maintenance
- Semantic commit types (`chore`)
- Template directories excluded from updates (`cli/src/command/template/**`)

---

## 6. Known Gaps and Missing Practices

### No PR Template or Review Checklist
There is no `.github/PULL_REQUEST_TEMPLATE.md` or `CONTRIBUTING.md`. Reviewers must rely on
their own judgment about what to check. The checklists in Section 4 above are intended to
fill this gap.

### No CODEOWNERS
No GitHub CODEOWNERS file exists. There is no automated routing of reviews based on file
paths. All changes are reviewed by whoever is available.

### Limited Unit Test Coverage
- **Rust:** Only one `#[cfg(test)]` module exists (in `cli/src/command/start/registry.rs`
  for archive cache behavior). No `#[test]` functions outside of that. The entire CLI and
  SDK Rust codebase has effectively zero unit test coverage.
- **Go:** Two test files exist (`context_test.go` and `context_auth_test.go`) covering
  artifact alias parsing and credential handling. No tests for artifact builders, the build
  pipeline, or store operations.
- **TypeScript:** No test files found.

This means reviewers must compensate for low test coverage by carefully checking correctness
manually, especially for edge cases and error paths.

### No Integration Test Suite
The CI parity check (comparing artifact digests across SDKs) is the closest thing to
integration testing. There are no tests that validate gRPC service interactions, auth flows,
or registry operations in isolation.

### 12 Open TODOs in Rust Code
Several TODO comments indicate incomplete work or known shortcuts:
- Parallel artifact preparation (`context.rs:337`)
- Lock file version checking (`context.rs:425`)
- Docker step secrets support (`step.rs:257`)
- Archive upload deduplication (`worker.rs:851`)
- Alias validation (`artifact/local.rs:96`)
- Registry artifact listing (`registry.rs:401`)

Reviewers should watch for PRs that touch code near these TODOs and evaluate whether the
TODO should be addressed as part of the change.

### `exit(1)` Instead of Error Propagation
The build command (`build.rs`) uses `exit(1)` in several error paths instead of propagating
errors via `Result`. This makes testing harder and can skip cleanup. Reviewers should flag
new instances of this pattern.

---

## 7. Commit Message Conventions

The project follows conventional commits based on recent history:

- `feat(scope):` — New features (`feat(agent,sdk): add artifact source and digest caching`)
- `fix(scope):` — Bug fixes (`fix(deps): correct terraform version to 1.14.6`)
- `chore(scope):` — Maintenance (`chore(deps): upgrade default artifact versions across all SDKs`)
- `refactor(scope):` — Code restructuring (`refactor(sdk): consolidate TLS client config`)

Renovate bot uses `chore(deps):` for automated updates. Reviewers should verify that commit
messages accurately reflect the change type (e.g., don't use `chore` for a behavioral change).

---

## 8. Review Workflow Recommendations

### Before Reviewing
1. Check which SDKs are affected — if one SDK is changed, check the other two
2. Read the proto definitions if any API-facing change is involved
3. Understand the artifact digest implications of any change to serialization or hashing

### During Review
1. Start with the highest-risk files (worker.rs, context.rs, auth.rs, build.rs)
2. Check error handling and resource cleanup
3. Verify cross-SDK consistency for artifact builder changes
4. Look for new `exit(1)` calls, `unwrap()`/`expect()` in non-test code, or missing auth checks
5. Check that environment variable expansion doesn't introduce injection risks

### After Review
1. Verify CI passes on all 4 platforms
2. Confirm SDK parity tests pass (the test job in CI)
3. For auth changes, consider manual testing against a real OIDC provider
