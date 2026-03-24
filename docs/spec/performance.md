---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-24"
updated_by: "@staff-engineer"
scope: "Performance characteristics, caching strategies, concurrency patterns, and known bottlenecks in the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - operations.md
---

# Performance

This document describes the performance characteristics of the Vorpal build system as they exist in the codebase today, including caching strategies, concurrency patterns, compression choices, streaming behavior, and known bottlenecks.

## 1. Async Runtime

Vorpal uses **Tokio** with the `rt-multi-thread` feature as its async runtime across all three workspace crates (`vorpal-cli`, `vorpal-sdk`, `vorpal-config`). The CLI entry point uses `#[tokio::main]` with default thread pool sizing (number of CPU cores).

There is no explicit runtime tuning — no custom worker thread counts, no blocking thread pool configuration, and no `runtime::Builder` customization.

## 2. gRPC Communication

All inter-service communication uses **tonic** (gRPC over HTTP/2). The system defines five gRPC services across four proto files:

| Service | Transport Pattern | Notes |
|---------|------------------|-------|
| `AgentService.PrepareArtifact` | Unary request, server-streaming response | Agent sends progress updates back to caller |
| `ArchiveService.Check` | Unary | Existence check with caching |
| `ArchiveService.Pull` | Unary request, server-streaming response | Archives streamed in chunks |
| `ArchiveService.Push` | Client-streaming request, unary response | Archives uploaded in chunks |
| `WorkerService.BuildArtifact` | Unary request, server-streaming response | Build output streamed back |
| `ContextService` | Unary RPCs | Config server for artifact metadata |
| `ArtifactService` | Unary RPCs | Artifact metadata CRUD |

### Chunk Sizes

- **Archive pull (registry to client)**: `DEFAULT_GRPC_CHUNK_SIZE = 2 * 1024 * 1024` (2 MB) — defined in `cli/src/command/start/registry.rs:44`
- **Archive push (agent/worker to registry)**: `DEFAULT_CHUNKS_SIZE = 8192` (8 KB) — defined in both `cli/src/command/start/agent.rs:55` and `cli/src/command/start/worker.rs:51`
- **Duplex I/O bridge buffer**: `DUPLEX_BUF_SIZE = 256 * 1024` (256 KB) — defined in `cli/src/command/store.rs:8`, used for xz decompression bridging

**Gap**: The push chunk size (8 KB) is significantly smaller than the pull chunk size (2 MB). This asymmetry means uploads require far more gRPC frames than downloads for the same payload, increasing per-message overhead on the push path.

### Channel Construction

gRPC channels are created via `build_channel()` in `sdk/rust/src/context.rs`. For Unix domain sockets, `connect_with_connector_lazy` is used — the connection is deferred until the first RPC call, avoiding startup races. For TCP connections, `endpoint.connect()` eagerly establishes the connection.

There is no connection pooling, channel reuse strategy, or keepalive configuration. Each service interaction creates a new channel via `build_channel()`.

### mpsc Channel Buffer

Both the agent and worker use `mpsc::channel(100)` for streaming response channels. This provides backpressure with a 100-message buffer before the sender blocks.

## 3. Caching Strategies

### 3.1 Archive Check Cache (Registry Server)

**Location**: `cli/src/command/start/registry.rs`

The `ArchiveServer` uses a **moka** async cache (concurrent, lock-free, TTL-based) to cache archive existence check results. This avoids redundant backend calls (filesystem or S3) for repeated `Check` RPCs.

- **Cache key**: `"{namespace}/{digest}"`
- **TTL**: Configurable via `--archive-cache-ttl` CLI flag (passed as `archive_cache_ttl: u64` seconds)
- **TTL=0**: Effectively disables caching (immediate expiry)
- **Negative caching**: "Not found" results are also cached, preventing repeated backend lookups for missing archives
- **Scope**: Per-server-process, in-memory only — not shared across restarts

This cache has unit tests covering: hit/miss behavior, namespace isolation, negative caching, TTL expiration, and TTL=0 disable mode.

### 3.2 Source Digest Cache (Agent)

**Location**: `cli/src/command/start/agent.rs`

The `AgentServer` maintains a session-scoped `SourceCache` (an `Arc<Mutex<SourceCacheState>>`) that caches computed source digests for HTTP sources only. Local sources are always re-read from disk.

- **Two-level lookup**: `by_key` (full source fields + platform) and `by_url` (URL + excludes + includes + platform)
- **Backfill**: If a URL-cache hit resolves a digest, the full-key cache is also populated
- **Scope**: Per-`AgentServer` instance, in-memory, lives for the duration of the server process
- **No TTL**: Entries never expire within a session — appropriate since HTTP source content at a given URL is expected to be immutable within a build session

### 3.3 Artifact Input Cache (SDK Context)

**Location**: `sdk/rust/src/context.rs`

`ConfigContext` maintains an `artifact_input_cache: HashMap<String, String>` that maps the SHA-256 digest of an artifact's JSON representation (the "input digest") to its "output digest" (the digest returned by the agent after source preparation). This avoids redundant `PrepareArtifact` RPCs for identical artifact definitions within a single build session.

### 3.4 Local Artifact Output Cache

The build system uses a content-addressable store on disk at `/var/lib/vorpal/store/artifact/output/{namespace}/{digest}`. Before triggering a build, the `build()` function in `cli/src/command/build.rs` checks if the output directory already exists. If it does, the build is skipped entirely. This is the primary mechanism for incremental builds.

Similarly, before building, the system checks the archive store (`/var/lib/vorpal/store/artifact/archive/{namespace}/{digest}.tar.zst`). If an archive exists locally, it is unpacked directly without contacting the registry or worker.

### 3.5 JWK Set Cache (Auth)

**Location**: `cli/src/command/start/auth.rs`

The OIDC validator caches the JWK set from the issuer. If a token's `kid` is not found in the current set, it fetches a fresh set (comment: "refresh if key not found"). This is a simple cache-then-refresh pattern.

## 4. Compression

All artifact and source archives use **zstd compression** with tar (`tar.zst`):

- **Compression**: `async_compression::tokio::write::ZstdEncoder` wrapping a `tokio_tar::Builder` — uses default zstd compression level (no explicit level configuration)
- **Decompression**: `async_compression::tokio::bufread::ZstdDecoder` wrapping `tokio_tar::Archive`
- **Additional formats supported for HTTP sources**: gzip, bzip2, xz (via `liblzma`), and zip — but only for downloading/unpacking remote sources, not for the artifact store itself

For **xz decompression**, the system bridges async and blocking I/O using `tokio::task::spawn_blocking` with a `tokio::io::duplex(DUPLEX_BUF_SIZE)` pipe, running decompression and tar extraction concurrently via `tokio::join!`.

## 5. Build Execution Model

### 5.1 Sequential Build Order

Artifact builds execute **sequentially** in topological order. The dependency graph is constructed using **petgraph** (`DiGraphMap`) with topological sort (`cli/src/command/config.rs:377`). Each artifact is built one at a time in the `build_artifacts` loop (`cli/src/command/build.rs:321`).

**Gap**: There is a `// TODO: make this run in parallel` comment in `sdk/rust/src/context.rs:337` for the `add_artifact` method, confirming that parallel artifact preparation is a known desired improvement.

### 5.2 Worker Build Steps

Within a single artifact, build steps also execute **sequentially** (`cli/src/command/start/worker.rs:799`). Each step spawns a child process (`tokio::process::Command`) with stdout/stderr merged via `StreamExt::merge` and streamed back to the caller line-by-line.

### 5.3 Source Pulling

Sources within an artifact are pulled **sequentially** in the worker (`cli/src/command/start/worker.rs:749`). Dependency artifacts are also pulled sequentially (`cli/src/command/start/worker.rs:777`), though a `HashSet` deduplicates dependency digests to avoid pulling the same artifact twice.

## 6. Content-Addressable Storage

All storage is content-addressed using **SHA-256** digests:

- **Source digests**: Computed by hashing each file individually, then hashing the concatenation of all file hashes (`cli/src/command/store/hashes.rs`)
- **Artifact digests**: SHA-256 of the JSON-serialized `Artifact` protobuf message
- **File hashing**: Uses the `sha256` crate's `try_digest` (file-based) and `digest` (in-memory) functions

**Gap**: The file hashing in `get_files_digests` uses sequential iteration with `.map()` — no parallel file hashing. For artifacts with many source files, this could be a bottleneck.

## 7. File Timestamp Normalization

All files in the store have their timestamps set to Unix epoch (0, 0) using `filetime::set_file_times` (`cli/src/command/store/paths.rs:219`). This ensures reproducible builds by eliminating timestamp-based differences. This operation is applied:

- After unpacking archives
- After copying local source files
- Before computing source digests

The timestamp normalization iterates over files sequentially with individual `set_timestamps` calls.

## 8. Disk Space Management

The `system prune` command (`cli/src/command/system/prune.rs`) provides manual disk space reclamation across five categories:

- Artifact aliases
- Artifact archives
- Artifact configs
- Artifact outputs
- Sandboxes

Directory sizes are calculated using `WalkDir` via `spawn_blocking` to avoid blocking the async runtime. There is no automatic garbage collection, LRU eviction, or disk space threshold monitoring.

## 9. Network Performance

### 9.1 Archive Transfer

Archive data is collected fully into memory before writing to disk. Both the build pull path (`cli/src/command/build.rs:113`) and worker pull path (`cli/src/command/start/worker.rs:186`) accumulate the entire gRPC stream into a `Vec<u8>` before writing. This means large artifacts must fit entirely in memory during transfer.

### 9.2 HTTP Source Downloads

HTTP source downloads use `reqwest::get` which downloads the entire response body into memory (`response.bytes().await`). For large source archives (e.g., toolchain downloads), this requires the full payload in memory before decompression begins.

**Exception**: xz decompression uses a streaming pipeline via `tokio::io::duplex`, allowing concurrent decompression and tar extraction.

### 9.3 S3 Backend

The S3 backend uses the AWS SDK (`aws-sdk-s3`) with default configuration (`BehaviorVersion::latest()`). There is no custom retry policy, timeout configuration, or multipart upload/download optimization visible in the registry backend code.

## 10. Known Bottlenecks and Improvement Opportunities

| Area | Current State | Impact |
|------|--------------|--------|
| Sequential artifact builds | Artifacts built one at a time despite DAG structure | Underutilizes available parallelism for independent artifacts |
| Push chunk size (8 KB) | Much smaller than pull chunk size (2 MB) | Excessive per-message overhead on uploads |
| Full-memory archive transfer | Entire archives buffered in memory | Memory pressure for large artifacts |
| Sequential source pulling | Sources and dependencies pulled one at a time | Slow for artifacts with many dependencies |
| No connection reuse | New gRPC channel per service interaction | Connection establishment overhead |
| No parallel file hashing | Source digest computed sequentially | Slow for large source trees |
| No automatic GC | Manual `system prune` only | Disk can fill up without operator intervention |
| No benchmarks | No criterion, `#[bench]`, or performance test suite | No regression detection for performance-sensitive paths |
| Default zstd level | No compression level tuning | May not be optimal for the artifact size/speed tradeoff |
| Sequential timestamp normalization | Each file's timestamp set individually | Adds up for large unpacked artifacts |

## 11. Benchmarking and Profiling

**There are no benchmarks, performance tests, or profiling configurations in the codebase.** No `criterion` dependency, no `#[bench]` annotations, no benchmark binary targets.

The `rust-toolchain.toml` uses `profile = "minimal"` for the toolchain itself, and Rust language builds use `--release` mode (`cargo build --release`).
