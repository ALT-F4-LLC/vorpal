# Review Strategy

This document describes the code review strategy for the Vorpal project. It identifies which review dimensions matter most, areas of high risk, common pitfalls, and what reviewers should focus on for different types of changes.

---

## Project Overview for Reviewers

Vorpal is a multi-language build system with a Rust CLI/server, a Rust SDK, a Go SDK (out-of-tree, generated via protobuf), and a TypeScript SDK. The system uses gRPC (tonic) for communication between CLI, agent, worker, and registry components. Artifacts are content-addressed by SHA-256 digest, with cross-SDK digest parity being a critical correctness requirement.

The codebase is approximately 22,000 lines across Rust (cli, config, sdk/rust) and TypeScript (sdk/typescript). There is no CONTRIBUTING.md, no PR template, and no formal review checklist today.

---

## Review Dimensions by Priority

The following dimensions are ranked by importance for this project. Every review should consider at least the top three; lower dimensions apply based on change type.

### 1. Correctness (Highest Priority)

**Why it matters most:** Vorpal is a build system. Incorrect builds, digest mismatches, or corrupted artifacts can silently produce wrong outputs that propagate downstream. A "wrong" build is worse than a failed build.

**What to verify:**
- Artifact digest computation must be deterministic and identical across all three SDKs (Rust, Go, TypeScript). The JSON serialization field order, field inclusion rules, and enum encoding must match exactly.
- Source digest computation (file hashing, timestamp sanitization) must be consistent.
- Lockfile hydration logic must correctly detect changed vs. unchanged sources.
- Config parsing (TOML) and resolution (layered settings: user, project, built-in defaults) must follow precedence rules.
- Build ordering via dependency graph (petgraph) must be topologically correct.

### 2. Security

**Why it matters:** The worker executes arbitrary shell scripts (`run_step` in `worker.rs`), the agent downloads and unpacks archives from HTTP URLs, and the system handles OAuth2 tokens and RSA keys for secret encryption/decryption.

**What to verify:**
- Any change to `worker.rs` `run_step()`: Does it expand environment variables safely? Does it prevent command injection through artifact names, digests, or environment values?
- Any change to `agent.rs` `build_source()`: Does it validate URLs and archive formats? Does it verify digests after download? Could a malicious archive path-traverse during unpacking?
- OIDC/JWT validation in `auth.rs`: Are issuer, audience, and expiration properly validated? Is the JWKS cache refresh safe against race conditions?
- Secret encryption/decryption via RSA (notary module): Are keys stored with appropriate permissions? Is the encryption padding scheme correct?
- Credential storage (`credentials.json`): Are tokens written with restrictive file permissions?
- `expand_env()` in `worker.rs`: Does variable expansion handle edge cases (overlapping prefixes, special characters) correctly?

### 3. Cross-SDK Parity

**Why it matters:** Vorpal's core value proposition is that configurations written in Rust, Go, or TypeScript produce byte-identical artifact digests. Any drift breaks cache sharing and build reproducibility.

**What to verify:**
- `serializeArtifact()` in `sdk/typescript/src/context.ts` must produce JSON identical to Rust's `serde_json::to_vec` for prost-generated structs: snake_case field names, proto field-number order, all fields always present, enums as integers, optional None as null.
- `computeArtifactDigest()` must use SHA-256 on the same byte representation.
- Shell script templates in `artifact.ts` (ProcessBuilder, ProjectEnvironmentBuilder, UserEnvironmentBuilder) must be character-for-character identical to their Rust counterparts in `sdk/rust/src/artifact.rs`. Even whitespace differences change the digest.
- `step.ts` `shell()` function must produce the same `ArtifactStep` structure as Rust `step::shell()`.
- `parseArtifactAlias()` must behave identically across all SDKs (validation, defaults, error messages).

### 4. Architecture & Design

**What to verify:**
- Does the change respect the existing component boundaries (CLI, Agent, Worker, Registry, SDK)?
- Does it maintain the clean separation between the SDK (used by config authors) and the CLI (internal machinery)?
- Are new protobuf messages backward-compatible? Field numbering must never reuse retired numbers.
- Does the change introduce circular dependencies between crates?

### 5. Operations & Reliability

**What to verify:**
- gRPC error handling: Does the change distinguish between retriable (UNAVAILABLE) and non-retriable (NOT_FOUND, INVALID_ARGUMENT) errors?
- Streaming: Are gRPC streams properly consumed to completion? Partial reads can cause resource leaks.
- File lock management: Does the change properly acquire and release advisory locks (`fs4`)? Does it handle the lock file lifecycle correctly (never delete the lock file itself; rely on advisory lock release on process exit)?
- Unix domain socket lifecycle: Stale socket detection, cleanup on shutdown, permission setting (0o660).
- Signal handling: Does the change respect SIGINT/SIGTERM graceful shutdown?

### 6. Performance

**What to verify:**
- Archive operations: Are large archives streamed or loaded entirely into memory? The current pattern loads full archives into memory (`Vec<u8>`), which is acceptable for typical artifact sizes but should be flagged if sizes grow.
- gRPC chunk size: The 8KB default (`DEFAULT_CHUNKS_SIZE`) is conservative; the registry uses 2MB chunks. Mixing these creates overhead.
- Moka cache TTL for archive check results: Changes to `archive_check_cache_ttl` affect the tradeoff between consistency and registry round-trip cost.

---

## Risk Map by File/Module

| File / Module | Risk Level | Reason |
|---|---|---|
| `cli/src/command/start/worker.rs` | **Critical** | Executes arbitrary scripts, handles secrets, manages build workspace lifecycle |
| `cli/src/command/start/agent.rs` | **Critical** | Downloads and unpacks remote archives, handles source digest verification and lockfile mutation |
| `cli/src/command/start/auth.rs` | **High** | OIDC/JWT validation, token refresh, client credentials exchange |
| `sdk/rust/src/context.rs` | **High** | Artifact digest computation, gRPC channel/TLS setup, credential management |
| `sdk/typescript/src/context.ts` | **High** | Cross-SDK parity for serialization and digest computation |
| `sdk/typescript/src/artifact.ts` | **High** | Shell script templates that must be character-identical to Rust |
| `cli/src/command/build.rs` | **High** | Build orchestration, config process lifecycle, artifact dependency resolution |
| `cli/src/command/start/registry.rs` | **Medium** | S3/local storage backends, archive push/pull, artifact alias management |
| `cli/src/command/start.rs` | **Medium** | Service startup, TLS configuration, socket binding, signal handling |
| `cli/src/command/config.rs` | **Medium** | Layered settings resolution (user + project + defaults) |
| `sdk/rust/src/artifact/language/*.rs` | **Medium** | Language builder implementations; script templates affect digest parity |
| `sdk/typescript/src/artifact/language/typescript.ts` | **Medium** | TypeScript language builder; Bun integration |
| `sdk/rust/api/*.proto` | **Medium** | Protobuf schema changes affect all SDKs and wire compatibility |
| `config/src/**` | **Low** | Vorpal self-build configuration; changes rarely affect core behavior |
| `.github/workflows/*.yaml` | **Low** | CI pipeline; changes are low-blast-radius but should be reviewed for correctness |
| `cli/src/command/store/**` | **Low** | File path utilities, hash computation, archive compression; stable and well-tested |

---

## Review Checklist by Change Type

### Protobuf Schema Changes (`.proto` files)

- [ ] No reuse of retired field numbers
- [ ] New fields have appropriate default values (zero-values for scalars, empty for repeated)
- [ ] `go_package` option updated if needed
- [ ] Rust SDK `build.rs` regeneration verified
- [ ] Go SDK `make generate` regeneration verified
- [ ] TypeScript SDK generated types updated
- [ ] Wire-compatibility with existing clients verified (no breaking rename or type change)

### Cross-SDK Changes (any SDK modification)

- [ ] JSON serialization field order matches proto field numbering
- [ ] All fields are always present in serialized output (no skip-if-default)
- [ ] Enum values serialize as integers, not strings
- [ ] Optional fields serialize as `null` when absent
- [ ] SHA-256 digest computed on identical byte representations
- [ ] Shell script templates are character-for-character identical across SDKs
- [ ] `parseArtifactAlias()` behavior is consistent across SDKs
- [ ] Parity tests pass (`bun test` in `sdk/typescript/src/__tests__/`)

### Security-Sensitive Changes

- [ ] No new `unsafe` blocks (currently none in the codebase; keep it that way)
- [ ] URL/path inputs validated before use (no path traversal, no SSRF)
- [ ] OAuth2 tokens not logged or included in error messages
- [ ] File permissions set appropriately for credentials, keys, and sockets
- [ ] Archive unpacking cannot escape the sandbox directory
- [ ] Secret values encrypted before transport, decrypted only at point of use
- [ ] JWT validation includes issuer, audience, and expiration checks

### Worker / Build Execution Changes

- [ ] Environment variable expansion handles overlapping prefixes correctly
- [ ] Script files created with proper permissions (0o755)
- [ ] Workspace and lock files cleaned up on both success and failure
- [ ] Build lock prevents concurrent builds of the same artifact
- [ ] Output files have timestamps sanitized for reproducibility
- [ ] Dependencies pulled before build step execution (topological order preserved)

### CLI Changes

- [ ] `clap` defaults and resolved settings interact correctly (explicit CLI flags win over config values)
- [ ] New subcommands or flags documented in help text
- [ ] Error messages include actionable remediation steps
- [ ] Exit codes are consistent (0 for success, 1 for failure)
- [ ] `tracing` log levels appropriate (INFO for user-facing, DEBUG/TRACE for internal)

### CI / Workflow Changes

- [ ] Matrix still covers all four target platforms (macos-latest, macos-latest-large, ubuntu-latest, ubuntu-latest-arm64)
- [ ] Cache keys include `Cargo.lock` hash
- [ ] Cross-SDK parity tests still compare Rust, Go, and TypeScript digests
- [ ] Release pipeline produces artifacts for all architectures
- [ ] Build provenance attestation still runs on tagged releases

---

## Common Pitfalls

### 1. Digest Parity Drift

The most common source of bugs is when a change to one SDK's serialization or script template is not mirrored in the other SDKs. The TypeScript SDK's `serializeArtifact()` must exactly replicate Rust's `serde_json::to_vec` output for prost structs. Even adding a single field to a proto message requires updating the serialization in all SDKs.

**Prevention:** Always run the parity test suite (`bun test` in `sdk/typescript/src/__tests__/`) and the CI parity step after any artifact-related change.

### 2. Shell Script Template Mismatch

The `ProcessBuilder`, `ProjectEnvironmentBuilder`, and `UserEnvironmentBuilder` classes in both Rust and TypeScript generate shell scripts that become part of the artifact definition. A single whitespace or newline difference changes the artifact digest.

**Prevention:** When modifying a script template in one SDK, diff the output character-by-character against the other SDK. The `sdk/typescript/src/__tests__/step-parity.test.ts` test catches some of these, but manual inspection is still required for new templates.

### 3. Lockfile Concurrency

The agent writes to `Vorpal.lock` immediately after preparing each HTTP source. If multiple agents run against the same project directory simultaneously, the lockfile can be corrupted. There is no file-level locking on the lockfile today (only advisory locking on the socket).

**Prevention:** Flag any change that modifies lockfile write patterns. Consider whether the change introduces new concurrent access paths.

### 4. Memory-Bounded Archive Handling

Archives are currently loaded entirely into `Vec<u8>` before being written to disk or pushed via gRPC. This works for typical artifact sizes (tens of MB) but could OOM for very large artifacts.

**Prevention:** If a change introduces a new archive-handling path, verify it uses streaming or has a documented size limit.

### 5. TLS Configuration Asymmetry

The client TLS config (`get_client_tls_config`) in `context.rs` uses a custom CA certificate if present, otherwise falls back to system roots. The server TLS config in `start.rs` requires explicit cert and key files. Changes to either side must be tested end-to-end.

**Prevention:** Test TLS changes with both the Unix domain socket (no TLS) and TCP (with TLS) transport modes.

### 6. Config Resolution Precedence

The layered settings system (`config.rs`) resolves values as: CLI flag > project config > user config > built-in defaults. The `apply_default()` helper compares parsed values to clap's hardcoded defaults to detect whether the user explicitly set a flag. This is fragile: if a clap default changes without updating `apply_default`, the precedence breaks silently.

**Prevention:** When modifying clap defaults or adding new settings, verify the `apply_default()` comparison values match.

---

## Review Effort by Change Size

| Change Size | Lines Changed | Review Strategy |
|---|---|---|
| **Trivial** | <20 lines | Verify intent, check for hidden parity impact, approve quickly |
| **Small** | 20-100 lines | Full review of affected dimension(s), verify parity if SDK-related |
| **Medium** | 100-500 lines | Structured review across all applicable dimensions, run parity tests |
| **Large** | 500+ lines | Request split if spanning multiple concerns; focus on high-risk modules first |

For large changes, review in this order:
1. Protobuf schema changes (affects all SDKs)
2. Security-sensitive code (auth, worker, agent)
3. Cross-SDK parity (serialization, script templates)
4. Core build logic (build.rs, context.rs)
5. Supporting infrastructure (config, CLI flags, CI)

---

## Gaps and Missing Pieces

1. **No PR template or review checklist in the repository.** Reviewers must rely on this document for guidance. Consider adding a `.github/pull_request_template.md`.

2. **No CONTRIBUTING.md.** There are no documented expectations for contributors regarding testing, commit conventions, or review process.

3. **No automated parity enforcement in CI for TypeScript.** The CI workflow validates Rust-Go parity in the `test` job, but TypeScript parity is not yet integrated into the main pipeline. The `Vorpal.ts.toml` config exists but the parity comparison is only in the manual `tests/typescript-integration.sh` script.

4. **No code owners file.** There is no `CODEOWNERS` to automatically route reviews to domain experts for high-risk modules (auth, worker, SDK parity).

5. **Limited Rust test coverage.** Unit tests exist only in `sdk/rust/src/context.rs` (artifact alias parsing). There are no Rust unit tests for build orchestration, config resolution, worker execution, or agent source handling.

6. **TypeScript SDK tests are comprehensive relative to the Rust SDK.** The TypeScript SDK has parity tests, export tests, template tests, and context tests in `sdk/typescript/src/__tests__/`, which is more thorough than the Rust side.

7. **No integration test harness.** The `tests/typescript-integration.sh` script is the only integration test, and it requires running Vorpal services manually. There is no automated end-to-end test that spins up services and runs a full build cycle.

8. **Renovate manages dependency updates** via `.github/renovate.json`, with weekly lock file maintenance. Template directories are excluded from renovate scans (`cli/src/command/template/**`).
