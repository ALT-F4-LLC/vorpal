---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Performance characteristics, caching strategies, data transfer patterns, and known bottlenecks in the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - operations.md
---

# Performance

This document describes the performance-relevant characteristics of the Vorpal build system as they exist in the codebase today. It covers caching strategies, data transfer patterns, concurrency model, compression, hashing, and known gaps.

## 1. Caching Strategies

### 1.1 Registry Archive Check Cache (Moka)

The `ArchiveServer` uses [moka](https://crates.io/crates/moka) (`moka::future::Cache`) to cache the results of archive existence checks (`check` RPC). This is the only application-level cache in the registry service.

- **Cache key**: `"{namespace}/{digest}"` (string)
- **Cache value**: `bool` (whether the archive exists)
- **TTL**: Configurable via `--archive-cache-ttl` CLI flag (seconds). Defaults vary by deployment; setting TTL to `0` disables caching entirely.
- **Negative caching**: Enabled. "Not found" results are cached with the same TTL, preventing repeated backend lookups for missing archives.
- **Eviction**: Time-based only (no size cap configured). The cache relies on moka's internal concurrent hash map with lock-free reads.
- **Location**: `cli/src/command/start/registry.rs` (`ArchiveServer::new`, `ArchiveService::check`)

### 1.2 Agent Source Cache (In-Memory HashMap)

The `AgentServer` maintains a per-session `SourceCache` (`Arc<Mutex<SourceCacheState>>`) that avoids re-downloading and re-hashing HTTP sources within the same server lifetime.

- **Cache structure**: Two `HashMap`s — `by_key` (keyed on all source fields except digest, plus platform) and `by_url` (keyed on URL + excludes/includes/platform).
- **Scope**: HTTP sources only. Local filesystem sources always re-read from disk to detect changes.
- **Lifetime**: Lives for the duration of the agent process. No TTL, no eviction, no size cap.
- **Backfill**: On a URL-cache hit, the full key is backfilled into `by_key` to ensure future lookups by the canonical key also hit.
- **Location**: `cli/src/command/start/agent.rs` (`SourceCacheState`, `SourceCacheKey`)

### 1.3 Artifact Input Cache (ConfigContext)

The `ConfigContextStore` maintains an `artifact_input_cache: HashMap<String, String>` that maps pre-agent input digests to post-agent output digests. This prevents re-submitting identical artifacts to the agent service.

- **Scope**: Per-build session within `ConfigContext`.
- **No persistence**: Cleared when the build process exits.
- **Location**: `sdk/rust/src/context.rs` (`ConfigContextStore`)

### 1.4 Filesystem-Level Caching

Build outputs and archives are stored on the local filesystem under `/var/lib/vorpal/`. Key paths:

- `artifact/output/{namespace}/{digest}/` — unpacked artifact outputs
- `artifact/archive/{namespace}/{digest}.tar.zst` — compressed archives
- `artifact/config/{namespace}/{digest}.json` — artifact metadata

The worker checks `artifact_output_path.exists()` before building, effectively treating the filesystem as a persistent cache. No garbage collection runs automatically; the `vorpal system prune` command is the manual eviction mechanism.

### 1.5 What Is NOT Cached

- **OIDC discovery/JWKS**: The `OidcValidator` fetches OIDC configuration on startup but caching behavior for JWKS key rotation depends on the underlying HTTP client.
- **gRPC channels**: Each RPC call in the worker creates a new `build_channel()` connection. There is no connection pooling or channel reuse across calls within a single build.
- **Lockfile reads**: The lockfile (`Vorpal.lock`) is re-read from disk on each artifact preparation call, not cached in memory.

## 2. Data Transfer and Streaming

### 2.1 gRPC Streaming Chunk Sizes

Three different chunk sizes are used across the system:

| Context | Constant | Size | Location |
|---------|----------|------|----------|
| Agent source push | `DEFAULT_CHUNKS_SIZE` | 8 KiB | `cli/src/command/start/agent.rs:55` |
| Worker artifact push | `DEFAULT_CHUNKS_SIZE` | 8 KiB | `cli/src/command/start/worker.rs:51` |
| Registry local pull | `DEFAULT_GRPC_CHUNK_SIZE` | 2 MiB | `cli/src/command/start/registry.rs:44` |

The 8 KiB chunk size used by the agent and worker is notably small for archive transfers, which can be multi-megabyte. The registry uses a more reasonable 2 MiB chunk size for local backend pulls. The S3 backend streams chunks as received from the AWS SDK, which uses its own internal chunking.

### 2.2 Duplex Buffer

XZ decompression uses a `tokio::io::duplex` bridge with a buffer size of 256 KiB (`DUPLEX_BUF_SIZE` in `cli/src/command/store.rs:8`). This bridges the synchronous `liblzma` decoder with the async tar unpacker via `SyncIoBridge` and `tokio::task::spawn_blocking`.

### 2.3 Channel Buffer Sizes

gRPC response streaming channels are created with a buffer of 100 messages:

- Agent: `channel(100)` in `AgentService::prepare_artifact`
- Worker: `mpsc::channel(100)` in `WorkerService::build_artifact`
- Registry pull: `mpsc::channel(100)` in `ArchiveService::pull`

### 2.4 S3 Transfer Patterns

- **Pull**: Uses streaming via `get_object().send().body` — data is streamed chunk-by-chunk through the gRPC response channel without loading the entire object into memory.
- **Push**: The entire archive is loaded into memory as `request.data` before uploading via `put_object()`. No multipart upload support exists.
- **Check**: Uses `head_object()` for existence checks. The S3 pull implementation performs a redundant `head_object()` before `get_object()`.

## 3. Compression

### 3.1 Zstandard (zstd)

The primary archive format is `tar.zst` (zstd-compressed tar). Used for:

- Source archive packing (agent -> registry)
- Artifact output packing (worker -> registry)
- Archive unpacking (registry -> worker, registry -> CLI)

Implementation uses `async-compression` crate with tokio integration. Default compression level (no explicit level set, uses library default). Compression and decompression are async and streamed.

### 3.2 Other Decompression Formats (Agent Only)

The agent handles source downloads in multiple formats:

- **gzip**: `async_compression::tokio::bufread::GzipDecoder` — async streaming
- **bzip2**: `async_compression::tokio::bufread::BzDecoder` — async streaming
- **xz**: `liblzma::read::XzDecoder` — synchronous, bridged to async via `spawn_blocking` + duplex channel
- **zip**: `async_zip::tokio::read::seek::ZipFileReader` — async, but requires seekable reader (entire file written to temp first)

All downloaded content is first loaded entirely into memory (`response.bytes().await`) before decompression begins.

## 4. Hashing and Digest Computation

### 4.1 SHA-256

All content addressing uses SHA-256 via the `sha256` crate:

- **File hashing**: `sha256::try_digest(path)` — reads entire file synchronously on the calling thread
- **Source digest**: Hash each file individually, concatenate hex strings, hash the concatenation
- **Artifact digest**: `sha256::digest(serde_json::to_vec(&artifact))` — hash the JSON serialization

### 4.2 Performance Characteristics

- File hashing is **synchronous and blocking** (`try_digest` reads from disk on the current thread). For large source trees, this blocks the tokio runtime.
- Source digest computation iterates files sequentially with no parallelism.
- The digest-of-digests approach (concatenate hex strings, hash again) is simple but means the full list of per-file hashes must be held in memory.

## 5. Concurrency Model

### 5.1 Tokio Runtime

The CLI uses `tokio` with the multi-thread runtime (`rt-multi-thread` feature). No explicit thread pool sizing is configured; tokio uses its defaults (typically number of CPU cores).

### 5.2 Build Ordering

Artifact builds are executed **sequentially** following a topological sort of the dependency DAG:

- The DAG is built using `petgraph::graphmap::DiGraphMap`
- `petgraph::algo::toposort` produces a linear order
- Builds execute one at a time in `build_artifacts()` — there is no parallel execution of independent artifacts

A `TODO` comment in `context.rs:337` acknowledges this: `// TODO: make this run in parallel`.

### 5.3 Worker Step Execution

Within a single artifact build, steps execute sequentially. Each step spawns an external process (`tokio::process::Command`) with stdout/stderr merged via `StreamExt::merge`. There is no concurrent step execution.

### 5.4 Source Processing

Sources within an artifact are processed sequentially in `prepare_artifact()`. Each source may involve downloading, decompressing, hashing, and uploading — all done one at a time.

### 5.5 Locking

- **Agent source cache**: `tokio::sync::Mutex` — async-aware but serializes all cache access.
- **Worker artifact lock**: File-based lock (`get_artifact_output_lock_path`) prevents concurrent builds of the same artifact digest.
- **Server instance lock**: `fs4::fs_std::FileExt::try_lock_exclusive` on a lock file prevents multiple server instances.

## 6. Build Pipeline Critical Path

A typical build flows through these stages, each of which is a potential bottleneck:

1. **Config compilation** — The Vorpal config (Go/Rust/TypeScript) is compiled into a binary. This is itself a full build cycle through the agent/worker pipeline.
2. **Source preparation** (agent) — Download, decompress, hash, and upload each source. Sequential per source.
3. **Dependency resolution** — Topological sort and sequential build of all transitive dependencies.
4. **Artifact build** (worker) — Pull sources, pull dependency artifacts, run build steps, pack output, push to registry.
5. **Post-build pull** — CLI pulls the built artifact archive back from the registry and unpacks locally.

The entire pipeline is sequential at every level: sources within an artifact, steps within a build, and artifacts within the dependency graph.

## 7. Known Bottlenecks and Gaps

### 7.1 No Parallel Artifact Builds

The most impactful performance gap. Independent artifacts in the dependency DAG are built sequentially even when they share no dependencies. This is explicitly called out as a TODO.

### 7.2 Synchronous File Hashing

`sha256::try_digest()` performs blocking I/O on the async runtime. For large source trees with many files, this can starve other async tasks. Should use `spawn_blocking` or an async-aware hashing approach.

### 7.3 Small gRPC Chunk Sizes

The 8 KiB chunk size used by agent and worker for archive transfers results in high per-message overhead for large archives. The registry's 2 MiB chunk size is more appropriate. These should be unified.

### 7.4 Full In-Memory Downloads

HTTP source downloads load the entire response body into memory before processing. For large sources (e.g., toolchain tarballs), this creates memory pressure spikes.

### 7.5 No Connection Pooling

Each gRPC client in the worker (`ArchiveServiceClient`, `ArtifactServiceClient`) is created fresh for each build. Channel creation involves TCP/TLS handshakes that add latency.

### 7.6 Redundant S3 Operations

The S3 archive pull implementation calls `head_object()` before `get_object()`, adding an unnecessary round trip. The `get_object()` call itself will return a "not found" error if the object doesn't exist.

### 7.7 No Benchmarking Infrastructure

There are no benchmarks, no criterion tests, no flamegraph integration, and no performance regression tracking. The `Cargo.toml` has no `[bench]` section.

### 7.8 No Incremental Source Hashing

Source digests are recomputed from scratch on every build (for local sources). There is no mechanism to detect unchanged files and skip re-hashing.

### 7.9 Lockfile Re-reads

The lockfile is loaded from disk on every `prepare_artifact` call rather than cached in memory. For builds with many artifacts, this creates redundant disk I/O.

## 8. Disk Space Management

The `vorpal system prune` command (`cli/src/command/system/prune.rs`) is the only mechanism for reclaiming disk space. It supports selective pruning of:

- Artifact aliases
- Artifact archives
- Artifact configs
- Artifact outputs
- Sandboxes

Pruning calculates directory sizes via `walkdir` (offloaded to `spawn_blocking`), then removes and recreates the directory. There is no automatic garbage collection, no LRU eviction, and no disk space monitoring.

## 9. Network Considerations

### 9.1 Transport

- **Local**: Unix domain sockets (default when no `--port` specified). Lower overhead than TCP for co-located services.
- **Remote**: TCP with optional TLS. No HTTP/2 keep-alive or connection multiplexing configuration beyond tonic defaults.

### 9.2 Authentication Overhead

When OIDC auth is enabled, each RPC call in the build path calls `client_auth_header()`, which:
1. Reads credentials from disk
2. Checks token expiry
3. Potentially refreshes the token (OIDC discovery + token exchange)

There is no in-memory token cache — the credentials file is read on every call.

### 9.3 Retry Behavior

The config server connection has a simple retry (3 attempts, 500ms delay). No other RPC calls have retry logic. Failed builds terminate immediately.
