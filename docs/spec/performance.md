---
project: "vorpal"
maturity: "experimental"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Performance characteristics, caching strategies, concurrency patterns, and known bottlenecks in the Vorpal build system"
owner: "@staff-engineer"
dependencies:
  - architecture.md
  - operations.md
---

# Performance Specification

## 1. Overview

Vorpal is a distributed build system with a CLI client, agent service, worker service, and registry service. Performance-critical paths include artifact source preparation, archive compression/decompression, gRPC streaming for archive transfer, content-addressed hashing, and DAG-based build ordering. The system is built on Tokio's multi-threaded async runtime and uses gRPC (tonic) for all inter-service communication.

## 2. Async Runtime

The CLI binary uses `#[tokio::main]` with default settings (`cli/src/main.rs:5`), which configures Tokio's multi-threaded runtime with worker threads equal to the number of CPU cores. There is no custom runtime tuning (thread count, stack size, blocking thread pool) anywhere in the codebase.

**Gap:** No runtime configuration is exposed to operators. For large builds with many concurrent sources, the default blocking thread pool and channel buffer sizes may become bottlenecks without the ability to tune them.

## 3. Caching Strategies

### 3.1 Archive Check Cache (Registry Service)

The `ArchiveServer` uses the `moka` crate's async `Cache` for caching archive existence checks (`cli/src/command/start/registry.rs:105`). Key characteristics:

- **Cache key:** `"{namespace}/{digest}"` string
- **TTL:** Configurable via `--archive-cache-ttl` CLI flag (passed as `archive_cache_ttl: u64` seconds)
- **TTL=0 behavior:** Creates a cache with `Duration::ZERO` TTL, effectively disabling caching
- **Scope:** Only caches the boolean result of `check()` calls (archive exists or not), not archive data
- **Eviction:** Relies on moka's built-in TTL-based eviction; no max-size bound is configured

This cache prevents repeated `head_object` calls to S3 (or local filesystem stat calls) for archives that have already been verified to exist. The cache is per-server-instance and not shared across restarts.

### 3.2 Source Digest Cache (Agent Service)

The agent maintains an in-memory source digest cache (`cli/src/command/start/agent.rs:426-435`) that avoids re-downloading and re-hashing HTTP sources within a single server session:

- **Structure:** `SourceCacheState` with two lookup maps:
  - `by_key: HashMap<SourceCacheKey, String>` — keyed by (name, path, includes, excludes, platform)
  - `by_url: HashMap<(url, excludes, includes, platform), String>` — URL-based secondary index for HTTP sources
- **Scope:** HTTP sources only; local filesystem sources are always re-read from disk (correct behavior since local files may change)
- **Concurrency:** Protected by `Arc<Mutex<SourceCacheState>>` (Tokio mutex)
- **Lifetime:** In-memory only, cleared on server restart

### 3.3 Artifact Input Cache (SDK Context)

The `ConfigContextStore` contains an `artifact_input_cache: HashMap<String, String>` (`sdk/rust/src/context.rs:35`) that maps artifact input digests to output digests. This prevents re-submitting identical artifacts to the agent service within a single build session.

### 3.4 Lockfile-Based Digest Hydration

The agent loads `Vorpal.lock` at build time and hydrates source digests from it when sources have not changed (`cli/src/command/start/agent.rs:480-523`). This skips source download and hashing entirely for locked, unchanged sources, providing significant speedup for incremental builds.

### 3.5 Local Artifact Output Cache

The build command checks `get_artifact_output_path(digest, namespace)` before attempting any pull or build operation (`cli/src/command/build.rs:80-82`). If the artifact output directory already exists locally, the build is skipped entirely. This is the primary mechanism for incremental builds.

**Gap:** There is no cache size management, eviction policy, or garbage collection for locally cached artifacts. The `vorpal system prune` command exists but must be invoked manually.

## 4. Compression and Archive Performance

### 4.1 Zstandard (zstd) Compression

All artifact archives use zstd compression via the `async-compression` crate with Tokio integration (`cli/src/command/store/archives.rs:3`):

- **Compression:** `ZstdEncoder` wrapping a `tokio_tar::Builder` — streaming, async compression
- **Decompression:** `ZstdDecoder` wrapping a `BufReader` fed to `tokio_tar::Archive` — streaming, async decompression
- **Compression level:** Default (not explicitly configured); zstd defaults to level 3
- **Temp file pattern:** Compression writes to a temp file then copies to the destination, adding one extra I/O pass

**Gap:** No configurable compression level. For large artifacts, higher levels would trade CPU time for smaller archives (reducing transfer time), while lower levels would speed up local builds. The extra copy from temp file to destination could be eliminated.

### 4.2 Additional Decompression Formats

The agent supports decompressing HTTP-sourced archives in multiple formats (`cli/src/command/start/agent.rs:203-235`):
- gzip via `async_compression::GzipDecoder`
- bzip2 via `async_compression::BzDecoder`
- xz/lzma via `liblzma::read::XzDecoder` (synchronous, blocking)
- zip via `async_zip::ZipFileReader`

**Gap:** The xz/lzma decoder is synchronous (`liblzma::read::XzDecoder`) and runs on the async runtime, potentially blocking the Tokio worker thread during decompression of large xz archives. This should use `tokio::task::spawn_blocking` or an async xz decoder.

## 5. gRPC Streaming and Data Transfer

### 5.1 Chunk Size

Archive data is transferred via gRPC streaming with a fixed chunk size of 8192 bytes (`cli/src/command/start/worker.rs:51`):

```rust
const DEFAULT_CHUNKS_SIZE: usize = 8192; // default grpc limit
```

This is the default gRPC message size limit. For large artifacts (tens or hundreds of megabytes), this results in a very high number of small messages.

**Gap:** The chunk size is hardcoded and very small relative to typical artifact sizes. Tonic supports configuring `max_decoding_message_size` and `max_encoding_message_size` up to several megabytes, which would significantly reduce per-message overhead for large transfers.

### 5.2 Streaming Patterns

Both the build client and worker accumulate entire streamed responses into memory before writing to disk:

```rust
// cli/src/command/build.rs:112-132
let mut stream_data = Vec::new();
loop {
    match stream.message().await {
        Ok(Some(chunk)) => stream_data.extend_from_slice(&chunk.data),
        ...
    }
}
```

**Gap:** Full archive contents are buffered in memory before writing. For large artifacts, this causes high memory usage proportional to artifact size. A streaming-to-disk approach would bound memory usage.

### 5.3 Channel Buffer Sizes

gRPC response streaming uses `mpsc::channel(100)` for both agent and worker services (`cli/src/command/start/worker.rs:976`, `cli/src/command/start/agent.rs:731`). This provides backpressure when the consumer falls behind but the buffer size is not configurable.

### 5.4 S3 Streaming

The S3 archive backend streams data from S3's `get_object` response body directly through the gRPC channel (`cli/src/command/start/registry/archive/s3.rs:52-60`), avoiding full buffering. However, the push path reads the entire archive into memory before uploading to S3 as a single `put_object` call (`cli/src/command/start/registry/archive/s3.rs:81-88`).

**Gap:** S3 push does not use multipart upload for large artifacts. The entire archive is loaded into memory and uploaded in a single request.

## 6. Content-Addressed Hashing

### 6.1 File Hashing

Source digests are computed using the `sha256` crate (`cli/src/command/store/hashes.rs`):

- `get_file_digest`: Hashes a single file using `sha256::try_digest` (reads entire file into memory)
- `get_files_digests`: Hashes multiple files sequentially (no parallelism)
- `get_source_digest`: Combines individual file digests by concatenating hash strings and re-hashing

**Gap:** File hashing is sequential and synchronous. For source trees with many files, parallel hashing across files would improve throughput. The `sha256::try_digest` function reads the entire file into memory; streaming hash computation would reduce memory pressure for large files.

### 6.2 Artifact Input Digest

Artifact identity is computed by serializing the entire `Artifact` protobuf to JSON and SHA-256 hashing it (`sdk/rust/src/context.rs:325`). This is fast for typical artifact definitions but involves a full JSON serialization pass.

## 7. Build Ordering and Parallelism

### 7.1 DAG-Based Topological Sort

Build order is determined by topological sort of a `petgraph::DiGraphMap` (`cli/src/command/config.rs:364-385`). Artifacts declare step dependencies, and the graph ensures dependencies are built before dependents.

### 7.2 Sequential Build Execution

Despite the DAG providing information about which artifacts could be built in parallel, builds execute sequentially:

```rust
// cli/src/command/build.rs:321
for artifact_digest in artifact_order {
    build(...).await?;
}
```

The `ConfigContext::add_artifact` method also processes artifacts sequentially with a `// TODO: make this run in parallel` comment (`sdk/rust/src/context.rs:337`).

**Gap:** This is the most significant performance bottleneck. Independent artifacts in the DAG are built one at a time. Parallel execution of independent builds would dramatically reduce end-to-end build time for projects with wide dependency trees.

## 8. Connection Management

### 8.1 gRPC Channel Creation

The `build_channel` function (`sdk/rust/src/context.rs:593-636`) creates tonic `Channel` instances:

- **Unix domain socket:** Uses `connect_with_connector_lazy` for deferred connection (avoids startup races)
- **TCP/TLS:** Uses `Channel::builder(uri).connect()` for eager connection
- **No connection pooling:** Each `build_channel` call creates a new channel. The worker creates new channels per-source-pull and per-artifact-push operation rather than reusing a shared channel

**Gap:** The worker creates a new `ArchiveServiceClient` and channel for every `pull_source` and `pull_artifact` call (`cli/src/command/start/worker.rs:152-160`, `277-284`). HTTP/2 multiplexing within tonic channels means a single channel can handle concurrent RPCs, so reusing channels would reduce connection overhead.

## 9. Auth Token Performance

### 9.1 Token Refresh

The `client_auth_header` function (`sdk/rust/src/context.rs:684-775`) reads the credentials file from disk, checks expiry, and potentially refreshes the token on every call. There is no in-memory caching of the access token between calls.

**Gap:** Credentials are read from disk on every authenticated request. An in-memory cache with expiry-aware refresh would eliminate repeated file I/O and JSON parsing.

### 9.2 OIDC Discovery

Token refresh performs an OIDC discovery request (`sdk/rust/src/context.rs:646-647`) to find the token endpoint on every refresh. The discovery document is not cached.

**Gap:** The OIDC discovery document changes very rarely. Caching it (even with a short TTL) would eliminate unnecessary HTTP roundtrips during token refresh.

## 10. Benchmarking and Profiling

**Gap:** The codebase contains no benchmarks, no criterion tests, no profiling infrastructure, and no performance regression testing. There are no metrics or histograms for tracking build times, compression ratios, transfer speeds, or cache hit rates in production.

## 11. Known Performance TODOs in Codebase

The following TODO comments indicate acknowledged performance-related gaps:

| Location | TODO |
|---|---|
| `sdk/rust/src/context.rs:337` | `// TODO: make this run in parallel` |
| `cli/src/command/start/agent.rs:450` | `// TODO: Check if artifact already exists in the registry` |
| `cli/src/command/start/worker.rs:851` | `// TODO: check if archive is already uploaded` |
| `cli/src/command/start/agent.rs:673` | `// TODO: explore using combined sources digest for the artifact` |

## 12. Summary of Performance Gaps

| Category | Gap | Severity |
|---|---|---|
| Build parallelism | Independent artifacts built sequentially despite DAG information | High |
| Memory usage | Full archive buffering in memory during gRPC transfers | Medium |
| gRPC chunk size | Hardcoded 8KB chunks for large artifact transfers | Medium |
| Connection reuse | New gRPC channels created per operation in worker | Medium |
| File hashing | Sequential, synchronous hashing of source files | Medium |
| xz decompression | Synchronous decoder on async runtime | Medium |
| Token caching | Credentials file read from disk on every request | Low |
| OIDC discovery | Discovery document not cached during token refresh | Low |
| Compression level | Not configurable; uses zstd default | Low |
| Benchmarking | No benchmarks or performance regression tests exist | Low |
| Cache management | No eviction or size limits on local artifact cache | Low |
