---
project: "vorpal"
maturity: experimental
last_updated: "2026-03-20"
updated_by: "@staff-engineer"
scope: "Caching strategies, concurrency patterns, and performance-critical paths"
owner: "@staff-engineer"
dependencies:
  - architecture.md
---

# Performance

## Overview

Vorpal's performance model centers on content-addressed caching at every layer: source archive deduplication, artifact digest memoization, and in-memory session caches. The system is designed for build avoidance -- unchanged inputs skip all work.

## Caching Strategies

### Content-Addressed Artifact Cache

Every artifact and source is identified by a SHA-256 digest. The build pipeline checks for existing artifacts before doing work:

1. **Source digest check** -- Before downloading or processing a source, the agent checks the registry for an existing archive with the same digest. If found, the download is skipped entirely.
2. **Artifact digest check** -- After preparing an artifact (computing its digest from serialized protobuf), the worker checks the registry before executing build steps.
3. **Lock file hydration** -- `Vorpal.lock` records source digests per platform. On subsequent builds, locked digests are used to skip source processing for HTTP sources.

### In-Memory Source Cache

The Agent service maintains a per-session in-memory cache (`SourceCache`) to avoid redundant work within a single build session:

```rust
struct SourceCacheState {
    by_key: HashMap<SourceCacheKey, String>,  // full source fields -> digest
    by_url: HashMap<(url, excludes, includes, platform), String>,  // URL-based lookup
}
```

- HTTP sources are cached by both full key and URL key
- Local sources are never cached (must re-read from disk for freshness)
- Cache is per-process -- not shared across builds

### Archive Existence Cache

The registry's archive service uses a TTL-based cache (`moka` crate) for archive existence checks:
- Default TTL: 300 seconds (configurable via `--archive-cache-ttl`)
- Avoids repeated S3 HEAD requests for recently checked digests
- Can be disabled by setting TTL to 0

### Vendor Cache (CI)

CI uses GitHub Actions cache for `target/` and `vendor/` directories, keyed by `{arch}-{os}-{Cargo.lock hash}`. This avoids re-downloading and re-compiling dependencies on each CI run.

## Concurrency Patterns

### Async Runtime

Vorpal uses `tokio` with `rt-multi-thread` feature. The CLI and services are fully async:
- gRPC services use `tonic` async handlers
- File I/O uses `tokio::fs` for non-blocking operations
- Archive unpacking uses `tokio_tar` for async streaming
- XZ decompression uses `tokio::task::spawn_blocking` to offload CPU-intensive work to the blocking thread pool

### Streaming

- Source archives are pushed to the registry in 8KB chunks via gRPC streaming (`DEFAULT_CHUNKS_SIZE = 8192`)
- Artifact preparation uses `mpsc::channel(100)` for backpressure-controlled response streaming
- Archive unpacking pipes decompression through `tokio::io::duplex` channels

### Advisory File Locking

The server uses `fs4` advisory file locks to prevent multiple instances from binding the same UDS socket. Stale socket detection handles crash recovery.

## Performance-Critical Paths

### Build Hot Path

1. Parse `Vorpal.toml` and resolve layered config
2. Compile and run SDK config program (external process)
3. Agent: prepare artifact (source resolution + digest computation)
4. Registry: check/push source archives
5. Worker: execute build steps
6. Registry: store artifact metadata

Steps 3-5 are the most performance-sensitive. Source resolution for HTTP sources involves network I/O, decompression, and file hashing.

### Source Digest Computation

`get_source_digest` in `cli/src/command/store/hashes.rs` computes SHA-256 over all source files. This is CPU-bound for large source trees. Files are sorted before hashing for determinism.

### File Timestamp Normalization

`set_timestamps` normalizes all file timestamps to epoch 0 to ensure content-addressed reproducibility. This walks every file in the source sandbox.

### Compression

- Source archives use zstd compression (`compress_zstd`)
- Distribution tarballs use gzip (`tar -czf`)
- Decompression supports gzip, bzip2, xz, and zip formats

## Benchmarking

No dedicated benchmarking infrastructure exists in the codebase. There are no `#[bench]` tests, no criterion benchmarks, and no performance regression testing in CI.

## Scaling Considerations

### Single-Process Architecture

All services run in a single process. This simplifies deployment but limits horizontal scaling:
- Agent, Archive, Artifact, and Worker services share a single tokio runtime
- No service mesh or load balancing support
- No connection pooling for S3 (uses default reqwest/aws-sdk settings)

### Registry Backend

- **Local backend**: Limited by single-machine disk I/O and capacity
- **S3 backend**: Scales with S3 infrastructure. No explicit S3 transfer acceleration or multipart upload optimization

### Build Parallelism

Build steps within a single artifact execute sequentially (one step at a time). Cross-artifact parallelism depends on the SDK program's structure -- the SDKs support concurrent artifact preparation via the gRPC streaming interface.

## Gaps and Areas for Improvement

- No benchmarking framework or performance regression testing
- No lazy loading of artifact metadata -- all sources are resolved eagerly
- No pagination for artifact listing endpoints
- No connection pooling configuration exposed to users
- No batching of registry requests (each source is checked/pushed individually)
- XZ decompression blocks via `spawn_blocking` -- could be parallelized for multi-stream archives
- JWKS validation uses `block_in_place` in the gRPC interceptor (synchronous in async context)
- No build step parallelism within a single artifact
- No incremental source digest computation (full re-hash on every build for local sources)
