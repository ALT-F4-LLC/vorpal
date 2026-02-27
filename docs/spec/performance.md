# Performance Specification

> Describes the performance characteristics, caching strategies, known bottlenecks, and scaling
> considerations for the Vorpal project as they exist today.

---

## 1. Architecture Overview (Performance Lens)

Vorpal is a build system with a client-server architecture communicating over gRPC. The primary
performance-critical path is the artifact build pipeline:

```
CLI (client) → Agent (source prep) → Worker (build execution) → Registry (archive storage)
```

All services run in a single `vorpal` binary process (co-located by default), communicating over
Unix domain sockets or TCP. The system is built with **Tokio** (multi-threaded async runtime) and
**Tonic** (async gRPC framework).

---

## 2. Async Runtime & Concurrency

### Tokio Runtime

- **Runtime**: `tokio::main` with `rt-multi-thread` feature — uses a work-stealing thread pool
  sized to the number of CPU cores by default.
- **No custom runtime configuration**: The CLI uses the default Tokio runtime settings. There are
  no tuned worker thread counts, blocking thread pool sizes, or scheduler configurations.

### Concurrency Patterns

| Pattern | Location | Details |
|---|---|---|
| `tokio::spawn` | Worker build (`worker.rs:983`), Archive pull (`registry.rs:213`), Health reporter (`start.rs:273`) | Fire-and-forget async tasks for streaming responses and background work |
| `mpsc::channel(100)` | Worker build, Agent prepare, Archive pull | Bounded channels (capacity 100) for gRPC server streaming responses |
| `Arc<Mutex<T>>` | Agent source cache (`agent.rs:407`) | Shared mutable state for the source digest cache across concurrent requests |
| `Arc<RwLock<T>>` | OIDC JWKS cache (`auth.rs:107`) | Read-heavy lock for cached JWK sets; refreshed on cache miss |
| `tokio::select!` | Signal handling (`start.rs:91`) | Graceful shutdown via SIGINT/SIGTERM |
| `block_in_place` | Auth interceptor (`auth.rs:268`) | Bridges sync tonic interceptor with async token validation — blocks the current Tokio worker thread |

### Concurrency Gaps

- **Sequential artifact builds**: The `build_artifacts` function in `build.rs:303` processes
  artifacts in topological order, one at a time. Dependencies are respected, but independent
  artifacts within the same dependency layer are **not** built in parallel. There is an explicit
  `// TODO: make this run in parallel` comment in `context.rs:337`.
- **Sequential source preparation**: Each source in `agent.rs` is prepared sequentially within a
  single `prepare_artifact` call, even when sources are independent.
- **No connection pooling**: Each gRPC client connection is created per-operation in several
  places (e.g., `build.rs:537-541` creates new channels for archive and worker clients). The
  `build_channel` function (`context.rs:593`) creates a fresh connection each time.
- **`block_in_place` in auth interceptor**: The OIDC token validation uses
  `tokio::task::block_in_place` inside a tonic interceptor (`auth.rs:268`). This blocks the
  current Tokio worker thread during JWT validation and potential JWKS refresh. Under high
  concurrency, this could cause thread starvation.

---

## 3. Caching Strategies

### Archive Check Cache (Moka)

- **Library**: `moka` (v0.12.13) with `future` feature — a concurrent, async-aware cache.
- **Location**: `ArchiveServer` in `registry.rs:102-106`.
- **Key format**: `"{namespace}/{digest}"` (string).
- **TTL**: Configurable via `--archive-cache-ttl` CLI flag. Default: **300 seconds** (5 minutes).
  Setting to `0` disables caching.
- **Scope**: Caches the `check` RPC result (exists/not-found) for archive digests. Both positive
  and negative results are cached (negative caching prevents repeated backend lookups for missing
  archives).
- **No size limit**: The cache has no explicit max capacity — entries are only evicted by TTL.
- **Invalidation**: No manual invalidation mechanism. A newly pushed archive will not be
  reflected in `check` results until the TTL expires.
- **Test coverage**: 7 unit tests covering cache hits, misses, negative caching, TTL expiration,
  and TTL=0 disable behavior (`registry.rs:504-713`).

### Agent Source Digest Cache

- **Type**: `Arc<Mutex<SourceCacheState>>` (in-memory HashMap).
- **Location**: `AgentServer` in `agent.rs:680-690`.
- **Structure**: Two lookup maps:
  - `by_key`: `SourceCacheKey` (name + path + includes + excludes + platform) → digest
  - `by_url`: `(url, excludes, includes, platform)` → digest (HTTP sources only)
- **Lifetime**: Session-scoped — persists for the lifetime of the agent server process.
- **No TTL or eviction**: Entries accumulate without limit.
- **Purpose**: Avoids re-downloading and re-hashing identical sources within the same session.

### SDK Artifact Input Cache

- **Type**: `HashMap<String, String>` inside `ConfigContextStore`.
- **Location**: `context.rs:35`.
- **Key**: Input digest (SHA-256 of serialized artifact JSON before agent processing).
- **Value**: Output digest (SHA-256 after agent processes sources).
- **Purpose**: Short-circuits `add_artifact` when the same artifact definition is submitted
  multiple times within a single config evaluation.

### OIDC JWKS Cache

- **Type**: `Arc<RwLock<JwkSet>>` in `OidcValidator`.
- **Location**: `auth.rs:107`.
- **Refresh strategy**: Cache-then-refresh. On validation, the current JWK set is tried first.
  If the `kid` is not found, JWKS is re-fetched from the provider and retried once.
- **No TTL**: The JWKS cache is only refreshed on key miss — it does not proactively refresh.

### Filesystem-Level Caching

- **Artifact output deduplication**: Before building, both the worker (`worker.rs:697-701`) and
  the build client (`build.rs:78-82`) check if `artifact_output_path` already exists on disk. If
  the output directory is present, the build is skipped entirely.
- **Archive deduplication**: Before pushing archives, the S3 backend checks `head_object` to
  avoid re-uploading (`archive/s3.rs:70-79`). The local backend checks file existence
  (`archive/local.rs:50-53`).
- **Lockfile hydration**: The agent uses `Vorpal.lock` to hydrate known source digests, allowing
  the `check` RPC to short-circuit source re-download for locked sources (`agent.rs:461-495`).

---

## 4. Data Transfer & Serialization

### gRPC Streaming

- **Archive push/pull**: Uses gRPC client/server streaming for large binary data (archives).
- **Chunk sizes**:
  - Worker → Registry push: **8,192 bytes** (8 KB) — `DEFAULT_CHUNKS_SIZE` in `worker.rs:51`.
  - Agent → Registry push: **8,192 bytes** (8 KB) — `DEFAULT_CHUNKS_SIZE` in `agent.rs:53`.
  - Registry local pull: **2,097,152 bytes** (2 MB) — `DEFAULT_GRPC_CHUNK_SIZE` in
    `registry.rs:44`.
- **Asymmetry**: Push uses 8 KB chunks while local pull uses 2 MB chunks. The small push chunk
  size is significantly below the default gRPC max message size (4 MB) and may cause excessive
  framing overhead for large archives.

### Archive Accumulation in Memory

- The worker accumulates the **entire archive response into memory** before writing to disk
  (`worker.rs:186-198`, `worker.rs:303-316`). For large artifacts, this means the full
  compressed archive must fit in memory. The same pattern exists in the build client
  (`build.rs:111-131`).
- There is no streaming-to-disk implementation for received archives.

### Compression

- **Algorithm**: Zstandard (zstd) via `async-compression` crate.
- **Compression level**: Default (not explicitly configured — `ZstdEncoder::new` uses the library
  default, typically level 3).
- **Format**: tar.zst (tar archive compressed with zstd).
- **Implementation**: Fully async using `tokio_tar::Builder` and `async_compression::tokio`.
- **Decompression**: Also async, using `ZstdDecoder` with `BufReader`.
- **Other formats supported**: gzip, bzip2, xz, zip — for HTTP source unpacking in the agent.

### Hashing

- **Algorithm**: SHA-256 via `sha256` crate.
- **Usage**: Content-addressable storage for artifacts and sources.
- **`get_source_digest`**: Hashes each file individually, then concatenates all hex digests and
  hashes the result. This is done **synchronously** (blocking) via the `sha256` crate's
  `try_digest` which reads the entire file.
- **Performance concern**: For source directories with many files, the sequential file-by-file
  hashing can be slow. Files are hashed synchronously on the async runtime.

---

## 5. Network & Transport

### Unix Domain Sockets (Default)

- Default transport for local communication. Socket at `/var/lib/vorpal/vorpal.sock`
  (configurable via `VORPAL_SOCKET_PATH`).
- Lower latency than TCP for local IPC — no TCP handshake, no Nagle's algorithm, no network
  stack overhead.
- `connect_with_connector_lazy` is used for UDS clients (`context.rs:602`) — the connection is
  deferred until the first RPC, avoiding startup races.

### TCP Transport

- Used when `--port` is specified or `--tls` is enabled.
- Default port: `23151` (when TLS is enabled).
- Health check on separate plaintext port: `23152` (configurable).

### TLS

- Optional TLS via `tonic` with `tls-ring` feature (ring crypto provider).
- Installed as default crypto provider at startup (`command.rs:275-277`).
- Client TLS uses system native roots or a custom CA certificate from
  `/var/lib/vorpal/key/ca.pem`.

### HTTP Client

- **Library**: `reqwest` with `rustls-tls` backend.
- **Usage**: Source downloads (agent), OIDC discovery, token exchange.
- **No connection reuse**: `reqwest::get()` and `reqwest::Client::new()` are called per-request
  in several places without reusing a client instance (e.g., `command.rs:539`,
  `auth.rs:116-122`, `auth.rs:241-247`). Each `Client::new()` creates a new connection pool.

---

## 6. Storage & I/O

### Store Layout

All persistent data lives under `/var/lib/vorpal/`:

```
/var/lib/vorpal/
├── key/                      # TLS certificates, credentials
├── sandbox/                  # Temporary build workspaces (UUID v7 named)
├── store/
│   └── artifact/
│       ├── alias/            # name → digest mappings
│       ├── archive/          # compressed tar.zst archives
│       ├── config/           # artifact JSON configurations
│       └── output/           # unpacked artifact outputs
└── vorpal.sock               # Unix domain socket
```

### File I/O Patterns

- **`tokio::fs`** used throughout for async file operations.
- **`walkdir`** (synchronous) used for directory traversal in `get_file_paths` (`paths.rs:173`).
  This blocks the async runtime for large directory trees.
- **`filetime`** used to set all file timestamps to epoch (0) for reproducible builds. This
  iterates over every file after unpacking — O(n) per file in each archive.
- **UUID v7** for sandbox directory names — monotonically increasing, time-ordered.

### Disk Usage Concerns

- **No automatic cleanup of archives**: Once an archive is downloaded to the local store, it
  persists indefinitely. The `system prune` command exists for manual cleanup.
- **Sandbox cleanup**: Build workspaces are cleaned up after each build, but failure paths may
  leave orphaned sandboxes.
- **Lock files**: Advisory file locks (`fs4`) used for single-instance enforcement. Lock file
  intentionally left on disk after shutdown (released by OS on process exit).

---

## 7. Build Pipeline Performance

### Critical Path

The build pipeline for an artifact follows this sequence:

1. **Config evaluation** — Run language-specific config binary to produce artifact definitions
2. **Source preparation** (Agent) — Download/copy sources, hash, compress, push to registry
3. **Dependency resolution** — Topological sort of artifact DAG
4. **Dependency builds** — Sequential build of each artifact in dependency order
5. **Target build** (Worker) — Pull sources, pull dependency artifacts, execute build steps
6. **Archive & push** — Compress output, push to registry

### Known Bottlenecks

| Bottleneck | Impact | Severity |
|---|---|---|
| Sequential artifact builds | Independent artifacts in the same DAG layer wait for each other | High |
| 8 KB push chunk size | Excessive gRPC framing overhead for large archives | Medium |
| Full archive in memory | Memory pressure proportional to largest artifact | Medium |
| Synchronous file hashing | Blocks async runtime during source digest computation | Medium |
| Synchronous `walkdir` | Blocks async runtime during directory traversal | Low-Medium |
| Per-request HTTP clients | No connection reuse for OIDC discovery, source downloads | Low |
| `block_in_place` in auth | Blocks Tokio worker thread during JWT validation | Low (auth off by default) |
| No parallel source prep | Sources within an artifact are prepared sequentially | Medium |

### Optimistic Fast Paths

- **Output exists**: If the artifact output directory already exists on disk, the entire
  build/pull pipeline is skipped.
- **Archive exists in registry**: The `check` RPC (with moka cache) avoids redundant archive
  pulls.
- **Lockfile digest hydration**: Known source digests from `Vorpal.lock` skip the download +
  hash + push cycle entirely.
- **SDK input cache**: Identical artifact definitions within a config evaluation return
  immediately.

---

## 8. Benchmarking & Profiling

### Current State

- **No benchmarks exist**: There are no `criterion` benchmarks, `cargo bench` targets, or
  performance test suites in the repository.
- **No profiling instrumentation**: The codebase uses `tracing` for structured logging (INFO
  level by default, DEBUG/TRACE available) but has no `#[instrument]` annotations, no span
  timing, and no metrics collection (no Prometheus, no OpenTelemetry).
- **No performance CI**: No automated performance regression detection.

### Observability

- **Logging only**: `tracing` + `tracing-subscriber` with configurable log level (`--level`).
  Outputs to stderr. DEBUG/TRACE include file and line numbers.
- **Health checks**: gRPC health service (`tonic-health`) available on a separate plaintext port.
  Reports service readiness but no performance metrics.
- **No metrics endpoint**: No Prometheus scrape target, no StatsD, no custom metrics.

---

## 9. Scaling Considerations

### Single-Process Model

The current architecture runs all services (agent, registry, worker) in a single process. This
means:

- **Vertical scaling only**: Performance scales with CPU cores (Tokio thread pool) and available
  memory.
- **No horizontal scaling for workers**: There is no built-in mechanism for distributing builds
  across multiple worker nodes.
- **Registry backends are pluggable**: Local filesystem and S3 are supported. S3 enables
  shared artifact storage across machines, but the worker is still single-node.

### Memory Scaling

- Archive accumulation in memory is O(archive_size) per concurrent build.
- Source cache grows unbounded over the server lifetime.
- Moka archive check cache grows unbounded (TTL-evicted only, no max entries).

### Disk Scaling

- Artifact storage grows monotonically. No automatic garbage collection.
- `system prune` provides manual cleanup with granular options (aliases, archives, configs,
  outputs, sandboxes).

### Multi-Platform Support

The system targets 4 platforms: `aarch64-darwin`, `aarch64-linux`, `x86_64-darwin`,
`x86_64-linux`. The worker validates that the build target matches the host system
(`worker.rs:658-665`), meaning cross-compilation requires platform-matched workers.

---

## 10. Gaps & Missing Pieces

| Gap | Description |
|---|---|
| No parallel builds | Artifacts within the same dependency layer are built sequentially |
| No streaming archive writes | Archives are fully accumulated in memory before disk write |
| No benchmark suite | No `criterion` or similar benchmarks for performance regression detection |
| No metrics/observability | No Prometheus, OpenTelemetry, or custom performance metrics |
| No connection pooling | gRPC channels and HTTP clients are created per-operation |
| No cache size limits | Agent source cache and moka archive cache grow without bounds |
| No distributed workers | Single-node worker model; no work distribution mechanism |
| No incremental builds | Entire artifacts are rebuilt if any input changes |
| Small push chunk size | 8 KB chunks for archive push cause excessive framing overhead |
| Synchronous I/O in async context | `walkdir` and `sha256::try_digest` block the Tokio runtime |
