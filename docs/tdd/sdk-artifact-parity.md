---
project: "vorpal"
maturity: "stable"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Replace all fetch-alias artifact calls in Go and TypeScript SDKs with native build definitions matching the Rust SDK"
owner: "@staff-engineer"
dependencies:
  - ../spec/architecture.md
  - ../spec/testing.md
---

# SDK Artifact Parity: Native Build Definitions for Go and TypeScript

## 1. Problem Statement

The Go and TypeScript SDKs use `FetchArtifactAlias()` / `fetchArtifactAlias()` to retrieve pre-built artifacts from the registry by alias (e.g., `"bun:1.3.10"`). This means these SDKs depend on a running registry that already has the artifact built and stored by the Rust SDK. If the Rust SDK has not built an artifact, or the registry is unavailable, Go and TypeScript builds fail.

The Rust SDK, by contrast, defines every artifact as a native build -- specifying the source URL, build script, aliases, and target systems inline. This makes Rust builds self-contained: they can build any artifact from source without depending on pre-existing registry state.

**Why now:** The project is approaching a stable multi-SDK story. Fetch-based aliases create a hidden dependency chain (TS/Go -> registry -> prior Rust build) that is fragile, hard to debug, and prevents independent SDK operation. Achieving parity now prevents this pattern from spreading further.

**Constraints:**
- No new artifacts. Only re-implement what currently exists as fetch calls.
- No changes to the Rust SDK. It is the reference implementation.
- No refactoring of non-fetch code (language builders, step utilities, environments).
- The Rust SDK's source URLs, versions, build scripts, and alias strings are the canonical specification.

**Acceptance Criteria:**
1. Every `FetchArtifactAlias()` call in Go SDK artifact files (`sdk/go/pkg/artifact/*.go`) is replaced with a native build definition that mirrors the corresponding Rust implementation.
2. Every `fetchArtifactAlias()` call in TypeScript SDK artifact files is replaced with a native build definition that mirrors the corresponding Rust implementation.
3. Artifact definitions (name, sources, steps, aliases, systems) are structurally identical across all three SDKs.
4. SHA-256 digests produced by building the same artifact on the same platform are identical across all three SDKs. This is verified by the existing CI cross-SDK hash matching step.
5. All existing tests continue to pass.
6. The `vorpal-shell` definitions in `sdk/typescript/src/vorpal.ts` and Go SDK equivalent are updated to use the new native artifact functions instead of fetch calls.

## 2. Context & Prior Art

### Current Architecture

The Rust SDK at `sdk/rust/src/artifact/` contains 28 native artifact build definitions. Each artifact follows a consistent pattern:

1. **Struct definition** with optional version/configuration fields
2. **Builder pattern** with `new()` + `with_*()` methods
3. **`build()` method** that:
   - Determines platform-specific source URLs and targets
   - Creates an `ArtifactSource` from a remote URL
   - Generates a shell script for extraction/compilation
   - Creates step(s) via `step::shell()`
   - Returns an `Artifact` with name, steps, systems, aliases, and sources

The Go SDK at `sdk/go/pkg/artifact/` has the builder infrastructure (`builder.go`) with `NewArtifact`, `NewArtifactSource`, `Shell`, and `GetEnvKey` -- all the primitives needed for native builds. It already uses these for `oci_image.go`. But 17 artifact files are one-liners that delegate to `context.FetchArtifactAlias()`.

The TypeScript SDK at `sdk/typescript/src/artifact.ts` has the same builder infrastructure (`Artifact`, `ArtifactSource`, `ArtifactStep`, `shell`). It already uses these for `OciImage`. But artifact resolution in `vorpal.ts` and the language builders relies on `context.fetchArtifactAlias()`.

### How Digest Parity Works

Artifact digests are SHA-256 hashes of the serialized JSON representation of the `Artifact` protobuf message. Parity requires that the serialized form is byte-identical across SDKs. This means:
- Same `name` string
- Same `sources` array (same name, path, digest, includes, excludes -- in the same order)
- Same `steps` array (same entrypoint, script text, arguments, artifacts, environments, secrets -- in the same order)
- Same `systems` array (same enum values in the same order)
- Same `aliases` array (same strings in the same order)
- Same `target` enum value

The existing CI pipeline (`.github/workflows/vorpal.yaml` test stage) already validates cross-SDK digest matching. This is the primary verification mechanism.

### Existing Patterns in Go and TypeScript

Both SDKs already have working native builds for reference:
- **Go:** `oci_image.go` uses `NewArtifact`, `NewArtifactSource`, `Shell`, `GetEnvKey`
- **TypeScript:** `OciImage` class in `artifact.ts` uses `Artifact`, `ArtifactSource`, `shell`, `getEnvKey`
- **Both:** Language builders (`language/go.go`, `language/go.ts`) demonstrate how to compose sources, environments, and build scripts

## 3. Alternatives Considered

### A. Status Quo: Keep Fetch Aliases (Rejected)

**Strengths:** No implementation work. Simple code in Go/TS.
**Weaknesses:** Fragile dependency on registry state. Go/TS SDKs cannot build independently. Version drift between what Rust builds and what Go/TS fetches is invisible until runtime. Blocks the project from a stable multi-SDK story.

### B. Shared Artifact Definition Files (Rejected)

Generate artifact definitions from a shared specification (YAML/JSON) consumed by all three SDKs.

**Strengths:** Single source of truth for versions, URLs, build scripts.
**Weaknesses:** Adds a code generation layer and a new artifact format. Significant infrastructure work. The Rust SDK's code IS the specification -- adding another layer of indirection creates more maintenance burden, not less. Would require changes to the Rust SDK (out of scope).

### C. Native Build Definitions (Recommended)

Port each Rust artifact's `build()` implementation to equivalent Go and TypeScript code using each SDK's existing builder primitives.

**Strengths:** Each SDK becomes self-contained. Uses existing infrastructure. No new abstractions or tools needed. The Rust code serves directly as the specification for the port. Verified by existing CI digest matching.
**Weaknesses:** Code duplication across three languages (inherent to multi-language SDKs). Version updates require changes in three places (already true for language builders).

## 4. Architecture & System Design

### Artifact Categories

Analysis of the 17 Go fetch artifacts and 14 TypeScript fetch artifacts reveals five distinct build patterns:

#### Pattern 1: Download + Extract Binary (7 artifacts)
Download a platform-specific archive, extract binary to `$VORPAL_OUTPUT/bin/`.

| Artifact | Source | Rust Reference |
|----------|--------|----------------|
| `bun` | GitHub release (zip) | `sdk/rust/src/artifact/bun.rs` |
| `gh` | GitHub release (zip/tar.gz per platform) | `sdk/rust/src/artifact/gh.rs` |
| `nodejs` | nodejs.org tarball | `sdk/rust/src/artifact/nodejs.rs` |
| `pnpm` | GitHub release (single binary) | `sdk/rust/src/artifact/pnpm.rs` |
| `protoc` | GitHub release (zip) | `sdk/rust/src/artifact/protoc.rs` |
| `protoc_gen_go` | GitHub release (tar.gz) | `sdk/rust/src/artifact/protoc_gen_go.rs` |
| `go` | go.dev tarball | `sdk/rust/src/artifact/go.rs` |

Common structure:
1. Map `ArtifactSystem` to platform-specific URL fragment
2. Create `ArtifactSource` with the download URL
3. Shell script: `mkdir -p $VORPAL_OUTPUT/bin`, `cp` binary, `chmod +x`
4. Create `Artifact` with aliases and sources

#### Pattern 2: Build from Source with Configure+Make (2 artifacts)
Download source tarball, run `./configure && make && make install`.

| Artifact | Source | Rust Reference |
|----------|--------|----------------|
| `git` | kernel.org tarball | `sdk/rust/src/artifact/git.rs` |
| `rsync` | samba.org tarball | `sdk/rust/src/artifact/rsync.rs` |

Note: `rsync` uses `./configure` with specific disable flags (`--disable-openssl`, `--disable-xxhash`, `--disable-zstd`, `--disable-lz4`).

#### Pattern 3: Go Build (6 artifacts)
Use the Go language builder to compile a Go project from source.

| Artifact | Source | Rust Reference |
|----------|--------|----------------|
| `crane` | go-containerregistry source | `sdk/rust/src/artifact/crane.rs` |
| `goimports` | golang.org/x/tools source | `sdk/rust/src/artifact/goimports.rs` |
| `gopls` | golang.org/x/tools source | `sdk/rust/src/artifact/gopls.rs` |
| `grpcurl` | grpcurl source (depends on protoc) | `sdk/rust/src/artifact/grpcurl.rs` |
| `staticcheck` | go-tools source | `sdk/rust/src/artifact/staticcheck.rs` |
| `protoc_gen_go_grpc` | grpc-go source | `sdk/rust/src/artifact/protoc_gen_go_grpc.rs` |

These use the `Go` language builder (Rust: `artifact::language::go::Go`, Go SDK: `language.NewGo`, TS SDK: `Go` class) which handles fetching the Go distribution, setting up `GOARCH`/`GOOS`/`GOPATH`, and running `go build`.

Note: `goimports` and `gopls` share a common source (`go.googlesource.com/tools`). The Rust SDK has a helper `go::source_tools()` that creates the source for both. Go and TypeScript SDKs will need the same pattern.

Note: `grpcurl` depends on `protoc` as an artifact dependency. The Rust implementation uses `Protoc::new().build(context).await?` to get the protoc digest, then passes it via `.with_artifacts(vec![protoc])`.

#### Pattern 4: Composite Artifact (1 artifact)
Combines multiple sub-artifacts with a shell script.

| Artifact | Dependencies | Rust Reference |
|----------|-------------|----------------|
| `rust_toolchain` | cargo, clippy, rust_analyzer, rust_src, rust_std, rustc, rustfmt | `sdk/rust/src/artifact/rust_toolchain.rs` |

This artifact builds 7 Rust compiler components and assembles them into a toolchain directory. The sub-components (cargo, clippy, etc.) are themselves download+extract artifacts from `static.rust-lang.org`.

**Important:** In the Go SDK, `rust_toolchain` is fetched as a single alias. The Rust SDK builds all 7 sub-components natively. For parity, the Go SDK needs to either: (a) build all 7 sub-components natively too, or (b) use the Go/TS language `Rust` builder which already handles this internally. Analysis of the Go SDK shows `rust_toolchain.go` exports `RustToolchainTarget()` and `RustToolchainVersion()` helpers that are used by the Rust language builder (`language/rust.go`). The fetch is only used when `RustToolchain()` is called directly. The Go language Rust builder (`language/rust.go`) already calls `RustToolchain()` internally, so making `RustToolchain()` native propagates through the builder.

#### Pattern 5: Linux System Artifact (1 artifact)
Complex system-level artifact with dependencies on `linux_vorpal` and `rsync`.

| Artifact | Dependencies | Rust Reference |
|----------|-------------|----------------|
| `linux_vorpal_slim` | linux_vorpal, rsync | `sdk/rust/src/artifact/linux_vorpal_slim.rs` |

The Rust implementation depends on `LinuxVorpal` (which is a massive multi-stage Linux distribution build) and `Rsync`. For Go/TS, `linux_vorpal_slim` currently fetches via alias. Since `linux_vorpal` itself is a Rust-only artifact (not fetched in Go/TS), and `linux_vorpal_slim` is always consumed as a pre-built artifact, this one remains a fetch by design -- it cannot be built from source in Go/TS because `linux_vorpal` is out of scope.

**Decision:** `linux_vorpal_slim` stays as a fetch alias. It depends on `linux_vorpal` which is a Rust-only artifact. Building it natively in Go/TS would require porting the entire Linux distribution build, which is out of scope and arguably should always be built by the Rust SDK.

### TypeScript File Organization

The TypeScript SDK currently has fetch calls in two locations:
1. `sdk/typescript/src/vorpal.ts` -- the Vorpal project's own build config (fetch calls for the development shell)
2. `sdk/typescript/src/artifact/language/go.ts` -- the Go language builder (fetches `git` and `go`)
3. `sdk/typescript/src/artifact/language/rust.ts` -- the Rust language builder (fetches `protoc`, `rust-toolchain`)
4. `sdk/typescript/src/artifact/language/typescript.ts` -- the TypeScript language builder (fetches `bun`)
5. `sdk/typescript/src/artifact.ts` -- `OciImage.build()` fetches `crane` and `rsync`

For artifacts that are currently inline fetch calls (not in dedicated files), each needs a dedicated artifact module. The recommended structure mirrors Go SDK's flat file layout:

```
sdk/typescript/src/artifact/
  bun.ts
  crane.ts
  gh.ts        (Go SDK only, not fetched in TS -- skip)
  git.ts
  go.ts        (the Go distribution, not the language builder)
  goimports.ts
  gopls.ts
  grpcurl.ts
  nodejs.ts
  pnpm.ts
  protoc.ts
  protoc_gen_go.ts
  protoc_gen_go_grpc.ts
  rsync.ts
  rust_toolchain.ts
  staticcheck.ts
  language/    (existing, unchanged)
    go.ts
    rust.ts
    typescript.ts
  step.ts      (existing, unchanged)
```

### Dependency Graph

```
                   rust_toolchain
                   /    |    \    \    \    \    \
               cargo clippy rust_analyzer rust_src rust_std rustc rustfmt
                    (all download+extract from static.rust-lang.org)

                   grpcurl
                     |
                   protoc

        goimports   gopls
              \      /
         go::source_tools (shared source helper)

        crane   protoc_gen_go_grpc   staticcheck
          |           |                  |
     (go builder) (go builder)      (go builder)
          |           |                  |
        go+git      go+git            go+git

   bun  gh  nodejs  pnpm  protoc  protoc_gen_go  go  rsync  git
   (all independent, no inter-artifact dependencies)
```

## 5. API Contracts

### Go SDK Public API (per artifact file)

Each artifact file exports a single function matching the existing signature:

```go
// Before (fetch):
func Bun(context *config.ConfigContext) (*string, error) {
    return context.FetchArtifactAlias("bun:1.3.10")
}

// After (native build):
func Bun(context *config.ConfigContext) (*string, error) {
    name := "bun"
    system := context.GetTarget()
    sourceTarget := /* platform map */
    sourceVersion := "1.3.10"
    sourcePath := fmt.Sprintf("https://github.com/oven-sh/bun/releases/download/bun-v%s/bun-%s.zip", sourceVersion, sourceTarget)
    source := NewArtifactSource(name, sourcePath).Build()
    stepScript := fmt.Sprintf(/* extraction script */)
    step, err := Shell(context, nil, nil, stepScript, nil)
    // ...
    return NewArtifact(name, []*api.ArtifactStep{step}, systems).
        WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
        WithSources([]*api.ArtifactSource{&source}).
        Build(context)
}
```

The function signature does not change. Callers are unaffected.

### TypeScript SDK Public API (per artifact file)

Each artifact file exports a build function or class:

```typescript
// New file: sdk/typescript/src/artifact/bun.ts
export async function buildBun(context: ConfigContext): Promise<string> {
    const name = "bun";
    const system = context.getSystem();
    const sourceTarget = /* platform map */;
    const sourceVersion = "1.3.10";
    const sourcePath = `https://github.com/oven-sh/bun/releases/download/bun-v${sourceVersion}/bun-${sourceTarget}.zip`;
    const source = new ArtifactSource(name, sourcePath).build();
    const stepScript = /* extraction script */;
    const step = await shell(context, [], [], stepScript, []);
    return new Artifact(name, [step], SYSTEMS)
        .withAliases([`${name}:${sourceVersion}`])
        .withSources([source])
        .build(context);
}
```

### Callers Updated

After native builds are in place, callers in language builders and project configs are updated:

```typescript
// Before (in vorpal.ts):
const bun = await context.fetchArtifactAlias("bun:1.3.10");

// After:
const bun = await buildBun(context);
```

```typescript
// Before (in artifact/language/go.ts):
const git = await context.fetchArtifactAlias("git:2.53.0");
const go = await context.fetchArtifactAlias("go:1.26.0");

// After:
const git = await buildGit(context);
const go = await buildGo(context);
```

## 6. Migration & Rollout

### Migration Strategy

Each artifact is migrated independently. The migration is a drop-in replacement: the function signature stays the same, only the implementation changes from a fetch call to a native build. This means:

1. No API breaking changes
2. No caller changes needed (same function name, same return type)
3. Each artifact can be migrated and verified independently
4. Rollback is trivial: revert to the fetch call

### Rollout

All changes ship in a single branch/PR since the verification is all-or-nothing (CI digest matching). However, implementation is phased to manage complexity and allow parallel work.

### Breaking Changes

None. The public API surface (function signatures) is unchanged.

### Rollback Plan

Revert the PR. Since fetch aliases still exist in the registry, reverting to fetch calls restores the previous behavior immediately.

## 7. Risks & Open Questions

### Risks

1. **Script whitespace sensitivity (HIGH):** Digest parity requires byte-identical shell scripts. Trailing newlines, spaces, and indentation must match the Rust SDK exactly. The Go SDK uses `fmt.Sprintf` and Go string literals; the TypeScript SDK uses template literals. Both must produce the same string as Rust's `formatdoc!` and `format!` macros.

   *Mitigation:* Generate the script string from each SDK, hex-dump it (`xxd` or equivalent), and diff the hex output to catch invisible whitespace differences. The CI digest matching catches any discrepancy at integration time, but hex-dump comparison provides fast feedback during development without requiring a full CI build.

2. **Source name parity (HIGH):** The `ArtifactSource.name` field is part of the digest input and determines the extraction directory path (e.g., `./source/{name}/`). Go and TypeScript must use the exact same source name as Rust.

   *Mitigation:* Cross-reference each implementation against the Rust source. Source names are simple strings (e.g., `"bun"`, `"crane"`).

3. **Platform mapping drift (MEDIUM):** Each artifact maps `ArtifactSystem` to platform-specific URL fragments. These mappings must be identical across SDKs.

   *Mitigation:* Direct port from Rust match arms. CI matrix covers all four platforms.

4. **Shared source helper for goimports/gopls (MEDIUM):** Both use `go::source_tools()` in Rust which provides a shared source from `go.googlesource.com/tools`. Go and TypeScript need equivalent shared helpers.

   *Mitigation:* Create a `sourceTools()` helper in both SDKs, mirroring the Rust function.

### Resolved Scope Decisions

1. **`linux_vorpal_slim` exclusion: CONFIRMED.** Stays as a fetch alias. It depends on `linux_vorpal` (Rust-only multi-stage Linux distribution build). Porting that would be massive scope creep with no practical benefit.

2. **Rust toolchain sub-components: IN SCOPE for BOTH Go AND TypeScript.** Full parity requires native builds in both SDKs. The Go SDK currently fetches `rust-toolchain:1.93.1` as a single alias; the TypeScript Rust language builder (`language/rust.ts`) fetches it at lines 545 and 747. Both need 7 sub-component artifacts (cargo, clippy, rust_analyzer, rust_src, rust_std, rustc, rustfmt) plus the assembler. Each sub-component is a download+extract from `static.rust-lang.org` following the same Pattern 1 as other artifacts.

3. **TypeScript `step.ts` fetch: OUT OF SCOPE.** The `shell()` function in `sdk/typescript/src/artifact/step.ts` fetches `library/linux-vorpal:latest` for Linux bwrap steps. This is step infrastructure with a different alias pattern (`library/` prefix), not an artifact definition.

4. **Go SDK `step.go` fetch: OUT OF SCOPE.** Similarly, `sdk/go/pkg/artifact/step.go` fetches `library/linux-vorpal:latest`. Same reasoning as #3.

## 8. Testing Strategy

### Primary Verification: Cross-SDK Digest Matching (Existing)

The CI pipeline already validates that all three SDKs produce identical artifact digests. This is the definitive test for parity. Per `docs/spec/testing.md`, the CI test stage:

1. Builds artifacts using the Rust SDK config
2. Builds the same artifacts using Go SDK and TypeScript SDK configs
3. Compares SHA-256 digests
4. Fails if any mismatch

This test already runs on a 4-runner matrix (macOS x86/ARM, Ubuntu x86/ARM).

### Development Verification

During development, each artifact should be verified individually:

1. Build the artifact with the Rust SDK, capture the digest
2. Build the same artifact with the modified Go/TypeScript SDK
3. Compare digests

If digests differ, compare the serialized JSON artifact definitions field by field to isolate the discrepancy.

### Regression Testing

- All existing CI tests must continue to pass
- No new test files are needed -- the CI cross-SDK comparison is the test

## 9. Observability & Operational Readiness

### Build Failure Diagnostics

When a digest mismatch is detected in CI, the debugging workflow is:

1. Retrieve the artifact JSON from each SDK (already available in CI logs)
2. Diff the JSON to find the divergent field
3. Common causes: script whitespace, source name, alias string, system enum order

### No Runtime Impact

These changes affect build-time artifact definitions only. There are no runtime, deployment, or observability concerns. The artifacts produced are identical to what the Rust SDK currently produces.

## 10. Implementation Phases

### Phase 0: Shared Helpers (S)
**Prerequisite for Phases 1-3. Not parallelizable with other phases.**

**Go SDK:**
- Add `sourceTools()` function in `sdk/go/pkg/artifact/gobin.go` (or a new `go_tools.go` file) mirroring Rust's `go::source_tools()`

**TypeScript SDK:**
- Add `sourceTools()` function mirroring Rust's `go::source_tools()`

### Phase 1: Independent Download+Extract and Configure+Make Artifacts (M)
**All artifacts in this phase can be implemented in parallel. No inter-artifact dependencies.**

| Artifact | Go File | TS File | Rust Reference | Pattern |
|----------|---------|---------|----------------|---------|
| `bun` | `bun.go` | `bun.ts` | `bun.rs` | Download zip, extract binary |
| `gh` | `gh.go` | -- (not fetched in TS) | `gh.rs` | Download zip/tar.gz, extract binary |
| `go` | `gobin.go` | `go.ts` | `go.rs` | Download tarball, copy directory |
| `nodejs` | `nodejs.go` | `nodejs.ts` | `nodejs.rs` | Download tarball, copy directory |
| `pnpm` | `pnpm.go` | `pnpm.ts` | `pnpm.rs` | Download single binary |
| `protoc` | `protoc.go` | `protoc.ts` | `protoc.rs` | Download zip, extract binary |
| `protoc_gen_go` | `protoc_gen_go.go` | `protoc_gen_go.ts` | `protoc_gen_go.rs` | Download tar.gz, extract binary |
| `git` | `git.go` | `git.ts` | `git.rs` | Download tarball, configure+make (Pattern 2) |
| `rsync` | `rsync.go` | `rsync.ts` | `rsync.rs` | Download tarball, configure+make (Pattern 2) |

**Estimated: 9 artifacts x 2 SDKs = 18 file changes (minus `gh` not in TS = 17). Each file is ~30-60 lines.**

### Phase 2: Go-Build Artifacts (M)
**Depends on Phase 1 (requires native `go` and `git` artifacts). Artifacts within this phase can be implemented in parallel, except `grpcurl` which depends on `protoc`.**

| Artifact | Go File | TS File | Rust Reference | Notes |
|----------|---------|---------|----------------|-------|
| `goimports` | `goimports.go` | `goimports.ts` | `goimports.rs` | Uses shared `sourceTools()` |
| `gopls` | `gopls.go` | `gopls.ts` | `gopls.rs` | Uses shared `sourceTools()` |
| `crane` | `crane.go` | `crane.ts` | `crane.rs` | Go build from source |
| `staticcheck` | `staticcheck.go` | `staticcheck.ts` | `staticcheck.rs` | Go build from source |
| `protoc_gen_go_grpc` | `protoc_gen_go_grpc.go` | `protoc_gen_go_grpc.ts` | `protoc_gen_go_grpc.rs` | Go build from source |
| `grpcurl` | `grpcurl.go` | `grpcurl.ts` | `grpcurl.rs` | Go build, depends on `protoc` |

**Estimated: 6 artifacts x 2 SDKs = 12 file changes. Each file is ~20-40 lines (delegates to Go language builder).**

### Phase 3: Composite Artifacts (L)
**Depends on Phase 1 (download+extract sub-components). Both Go AND TypeScript SDKs.**

| Artifact | Go File | TS File | Dependencies | Rust Reference |
|----------|---------|---------|-------------|----------------|
| `rust_toolchain` | `rust_toolchain.go` | `rust_toolchain.ts` | cargo, clippy, rust_analyzer, rust_src, rust_std, rustc, rustfmt (all new) | `rust_toolchain.rs` |

This requires implementing 7 new download+extract artifacts for the Rust compiler components in BOTH SDKs:
- `cargo` -- from `sdk/rust/src/artifact/cargo.rs`
- `clippy` -- from `sdk/rust/src/artifact/clippy.rs`
- `rust_analyzer` -- from `sdk/rust/src/artifact/rust_analyzer.rs`
- `rust_src` -- from `sdk/rust/src/artifact/rust_src.rs`
- `rust_std` -- from `sdk/rust/src/artifact/rust_std.rs`
- `rustc` -- from `sdk/rust/src/artifact/rustc.rs`
- `rustfmt` -- from `sdk/rust/src/artifact/rustfmt.rs`

Then `rust_toolchain` assembles them using a shell script identical to the Rust implementation. The TypeScript Rust language builder (`language/rust.ts`) fetches `rust-toolchain` at lines 545 and 747, so this must be native in TypeScript for full parity.

**Estimated: 8 files x 2 SDKs = 16 file changes. Sub-components within each SDK are parallelizable.**

### Phase 4: Caller Updates and Cleanup (M)
**Depends on Phases 1-3. Not parallelizable -- touches shared files. Note: TypeScript has 33 fetchArtifactAlias occurrences across 7 files, making this more involved than the Go side.**

Update all callers that currently use fetch aliases to use the new native build functions:

**Go SDK:**
- `sdk/go/pkg/artifact/language/go.go` -- `Git()` and `GoBin()` calls already route through the artifact functions, so these are automatically updated

**TypeScript SDK:**
- `sdk/typescript/src/vorpal.ts` -- Replace all `context.fetchArtifactAlias()` calls with native build function imports
- `sdk/typescript/src/artifact/language/go.ts` -- Replace `fetchArtifactAlias("git:...")` and `fetchArtifactAlias("go:...")` with native build calls
- `sdk/typescript/src/artifact/language/rust.ts` -- Replace `fetchArtifactAlias("protoc:...")` and `fetchArtifactAlias("rust-toolchain:...")` with native build calls
- `sdk/typescript/src/artifact/language/typescript.ts` -- Replace `fetchArtifactAlias("bun:...")` with native build call
- `sdk/typescript/src/artifact.ts` -- Replace `fetchArtifactAlias("crane:...")` and `fetchArtifactAlias("rsync:...")` in `OciImage.build()` with native build calls

### Phase 5: Verification (S)
**Depends on Phase 4.**

1. Run full CI pipeline on all four platform runners
2. Verify cross-SDK digest matching passes for all artifacts
3. Verify all existing tests pass
4. Verify `vorpal-shell`, `vorpal-container-image`, and other composite artifacts build successfully

## 11. Complexity Summary

| Phase | Scope | Size | Parallelizable |
|-------|-------|------|----------------|
| 0 | Shared helpers | S | No (prerequisite) |
| 1 | 9 download+extract/configure+make artifacts x 2 SDKs | M | Yes (all independent) |
| 2 | 6 Go-build artifacts x 2 SDKs | M | Yes (except grpcurl) |
| 3 | rust_toolchain + 7 sub-components x 2 SDKs | L | Yes (sub-components independent) |
| 4 | Caller updates (33 TS fetch calls across 7 files + Go) | M | No (shared files) |
| 5 | Verification | S | No (sequential) |

Total: ~49 file changes across Go and TypeScript SDKs (17 Phase 1 + 12 Phase 2 + 16 Phase 3 + 4 Phase 4 shared file edits). Each individual file change is small (20-60 lines) and mechanically derived from the Rust reference implementation.

### Consensus Record

Approved via vote DKT-V5 (score: 1.00, threshold: 0.75, 3/3 reviewers).
View details: `docket vote show DKT-V5` | Full result: `docket vote result DKT-V5 --json`
