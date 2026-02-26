# Performance Specification

This document describes the performance characteristics, caching strategies, known bottlenecks,
and scaling considerations for the Vorpal project as they actually exist in the codebase today.

---

## 1. Runtime and Concurrency Model

### Tokio Async Runtime

The CLI and all server components run on the Tokio multi-threaded async runtime
(`tokio::main` with `rt-multi-thread` feature). This applies to:

- **CLI** (`cli/src/main.rs`): Single `#[tokio::main]` entry point.
- **SDK** (`sdk/rust/Cargo.toml`): `tokio` with `process` and `rt-multi-thread`.
- **Config binary** (`config/Cargo.toml`): `tokio` with `rt-multi-thread`.

The default Tokio thread pool size (equal to the number of CPU cores) is used. There is no
explicit tuning of worker thread count, stack sizes, or blocking thread pool sizes.

### gRPC Server Concurrency

The gRPC server is built with `tonic` (0.14.x). The `tonic::transport::Server` handles concurrent
connections natively via Tokio's task model. Each incoming RPC spawns into Tokio's task system:

- **Agent `prepare_artifact`**: Uses `tokio::spawn` with a `channel(100)` for streaming responses
  (`cli/src/command/start/agent.rs:613-621`).
- **Worker `build_artifact`**: Uses `tokio::spawn` with a `mpsc::channel(100)` for streaming
  build output (`cli/src/command/start/worker.rs:976-998`).
- **Archive `pull`**: Uses `tokio::spawn` with a `mpsc::channel(100)` for streaming archive data
  (`cli/src/command/start/registry.rs:209-234`).

Channel buffer size of 100 messages is hardcoded across all streaming RPCs. This limits
backpressure propagation -- if a consumer is slow, the producer blocks after 100 buffered messages.

### Build Pipeline Concurrency

**Artifact builds are sequential, not parallel.** The `build_artifacts` function in
`cli/src/command/build.rs:303-356` iterates through the topologically sorted artifact order and
builds each artifact one at a time in a `for` loop. There is a `TODO` comment in
`sdk/rust/src/context.rs:329` acknowledging this: `// TODO: make this run in parallel`.

This means:
- Independent artifacts in the dependency graph that could be built concurrently are built serially.
- Build time scales linearly with the number of artifacts, not with the critical path length.

---

## 2. Caching Strategies

### Archive Check Cache (Moka)

The most significant caching mechanism is the archive check cache in the registry's
`ArchiveServer` (`cli/src/command/start/registry.rs:102-130`). It uses the `moka` crate
(v0.12.x) -- a high-performance concurrent cache inspired by Caffeine (Java).

**Configuration:**
- Cache key: `"{namespace}/{digest}"` string.
- Cache value: `bool` (whether the archive exists).
- TTL: Configurable via `--archive-check-cache-ttl` CLI flag, default 300 seconds (5 minutes).
- TTL of 0 disables caching (immediate expiry).
- No maximum entry count is configured -- the cache grows unbounded during the TTL window.
- Supports negative caching (not-found results are cached too).

**Impact:** This cache avoids redundant filesystem checks (local backend) or S3 `HeadObject`
calls (S3 backend) during a build run. Since a single build can check the same archive digest
multiple times across different artifact builds, this provides meaningful latency reduction.

**Test coverage:** The caching behavior is thoroughly tested in
`cli/src/command/start/registry.rs:504-713` with tests for cache hits, misses, negative caching,
TTL expiry, and TTL=0 disabling.

### Content-Addressable Store (Filesystem)

Artifacts are stored in a content-addressable filesystem layout under `/var/lib/vorpal/store/`:

```
store/artifact/
  archive/{namespace}/{digest}.tar.zst    # compressed archives
  config/{namespace}/{digest}.json        # artifact metadata
  output/{namespace}/{digest}/            # unpacked outputs
```

**Cache-by-existence pattern:** Before building an artifact, the system checks whether the output
directory already exists (`get_artifact_output_path`). If it does, the build is skipped entirely
(`cli/src/command/build.rs:78-82` and `cli/src/command/start/worker.rs:697-701`). This is the
primary build avoidance mechanism.

**Lock file pattern:** Before starting a build, a `.lock.json` file is created
(`cli/src/command/start/worker.rs:706-728`). This prevents concurrent builds of the same artifact
but also means a crashed build leaves a stale lock. The `--rebuild` flag removes both lock and
output files to force a rebuild.

### Source Digest Caching

Source files are content-addressed by computing SHA-256 digests of all source files:
- Individual file digests are computed via `sha256::try_digest` (reads entire file into memory)
  in `cli/src/command/store/hashes.rs:5-11`.
- All file digests are concatenated and re-hashed to produce a single source digest
  (`get_source_digest` at `cli/src/command/store/hashes.rs:33-42`).
- Before pushing a source archive, the agent checks if it already exists in the registry via the
  `check` RPC (`cli/src/command/start/agent.rs:89-106`).

### OIDC/JWKS Caching

The OIDC validator caches the JWK set in an `Arc<RwLock<JwkSet>>`
(`cli/src/command/start/auth.rs:106-107`). JWKS is fetched once at startup and re-fetched
on-demand only when a token's `kid` is not found in the cached set (key rotation handling).
There is no periodic refresh or TTL on the JWKS cache.

### OAuth2 Token Credential Caching

Client credentials are stored on disk at `/var/lib/vorpal/key/credentials.json` and refreshed
when the token has less than 5 minutes of validity remaining
(`sdk/rust/src/context.rs:638-649`). There is no in-memory token caching between requests --
the credentials file is read from disk on every authenticated request.

---

## 3. Data Transfer and Compression

### Archive Format: Zstandard (tar.zst)

All archives use `tar.zst` format (Zstandard compression via `async_compression::tokio`):

- **Compression** (`cli/src/command/store/archives.rs:14-63`): Default Zstd compression level
  (no custom level configured). Compression is streamed through `ZstdEncoder` -> `tokio-tar`
  builder. Temp files are used as intermediates.
- **Decompression** (`cli/src/command/store/archives.rs:65-102`): Streamed through `ZstdDecoder`
  -> `tokio-tar` archive reader. Entries are unpacked individually to handle symlink overwrites.

### gRPC Streaming Chunk Sizes

Two different chunk sizes are used:

- **Agent source push**: 8,192 bytes (8 KB) -- defined as `DEFAULT_CHUNKS_SIZE` in
  `cli/src/command/start/agent.rs:48`.
- **Registry archive pull**: 2,097,152 bytes (2 MB) -- defined as `DEFAULT_GRPC_CHUNK_SIZE` in
  `cli/src/command/start/registry.rs:44`.
- **Worker archive push**: 8,192 bytes (8 KB) -- uses the same `DEFAULT_CHUNKS_SIZE` as agent
  (`cli/src/command/start/worker.rs:51`).

The inconsistency between 8 KB and 2 MB chunk sizes is notable. The 8 KB chunk size for pushing
creates significantly more gRPC messages for large archives, which adds per-message overhead.

### In-Memory Archive Buffering

Archives are fully buffered in memory during transfer in several places:

- **Build pull** (`cli/src/command/build.rs:111-131`): `stream_data` is a `Vec<u8>` that
  accumulates the entire archive in memory before writing to disk.
- **Worker pull** (`cli/src/command/start/worker.rs:186-198`): Same pattern -- entire archive in
  memory.
- **Agent push** (`cli/src/command/start/agent.rs:346`): Archive file is read entirely into memory
  (`read(&source_sandbox_archive)`), then chunked and sent.
- **Worker push** (`cli/src/command/start/worker.rs:868`): Same pattern -- read entire archive
  file into memory before streaming.
- **Registry push** (`cli/src/command/start/registry.rs:243-255`): All incoming chunks are
  accumulated into a single `Vec<u8>` before writing to storage backend.

This means the maximum archive size is bounded by available memory. For large artifacts
(hundreds of MB or more), this can cause significant memory pressure.

### S3 Backend

The S3 backend (`cli/src/command/start/registry/archive/s3.rs`) uses the AWS SDK v1.x:

- **Pull**: Uses `get_object` with streaming body -- chunks are forwarded directly to gRPC
  without full buffering (good).
- **Push**: Receives the entire archive as a single `request.data` blob and uploads it with
  `put_object`. No multipart upload is used, so S3's 5 GB single-upload limit applies.
- **Check**: Uses `head_object` -- a lightweight metadata-only operation.
- **Idempotent push**: Before uploading, checks if the object already exists via `head_object`
  to avoid re-uploading.

No connection pooling configuration exists beyond what the AWS SDK provides by default.

---

## 4. Hashing and Digest Computation

### SHA-256 Everywhere

All content addressing uses SHA-256 via the `sha256` crate:

- **File digests**: `sha256::try_digest` reads the entire file into memory to hash it
  (`cli/src/command/store/hashes.rs:10`). For large files, this has O(n) memory cost.
- **Artifact digests**: The artifact struct is serialized to JSON, then SHA-256 hashed
  (`sdk/rust/src/context.rs:323`). This is fast for typical artifact metadata.
- **Source digests**: All source file digests are concatenated and re-hashed
  (`cli/src/command/store/hashes.rs:23-31`). Digest computation is sequential --
  no parallel file hashing.

### Cross-SDK Parity

The TypeScript SDK (`sdk/typescript/src/context.ts:137-140`) uses Node.js `createHash("sha256")`
with a custom JSON serializer (`serializeArtifact`) that matches Rust's `serde_json` output
field ordering. This is critical for cross-SDK digest parity but adds complexity -- any
divergence in serialization order produces different digests.

---

## 5. DAG Resolution and Build Ordering

### Topological Sort

Build ordering is computed using `petgraph::algo::toposort` over a `DiGraphMap`
(`cli/src/command/config.rs:374-395`). The graph nodes are artifact digest strings, and edges
represent step dependencies.

**Characteristics:**
- `toposort` runs in O(V + E) time, which is efficient.
- The graph is rebuilt from scratch on every build -- no incremental graph maintenance.
- Cycle detection is handled by `toposort` returning an error.

### Recursive Artifact Fetching

`fetch_artifact` in `sdk/rust/src/context.rs:399-443` recursively fetches artifact dependencies
using `Box::pin(self.fetch_artifact(dep))`. This is sequential -- each dependency is fetched one
at a time. For deep dependency trees, this creates a serial chain of gRPC calls.

### Artifact Store Cloning

The artifact store (`HashMap<String, Artifact>`) is cloned in several places:
- `get_artifact_store()` returns a full clone (`sdk/rust/src/context.rs:445-447`).
- `config_artifacts_store.clone()` during the selected artifact search
  (`cli/src/command/build.rs:704`).

For builds with many artifacts, these clones copy significant data unnecessarily.

---

## 6. File System Operations

### Timestamp Normalization

All files are normalized to epoch 0 timestamps using `filetime::set_file_times`
(`cli/src/command/store/paths.rs:214-226`). This is done:
- After unpacking archives (per-file loop).
- After copying source files.
- After downloading remote sources.

For archives with many files, this creates one syscall per file, which is I/O-bound on
traditional filesystems.

### File Walking

`walkdir::WalkDir` is used for directory traversal
(`cli/src/command/store/paths.rs:173-190`). The walker iterates synchronously (not async) and
applies include/exclude filters in-memory. For large source trees, the initial walk can be slow.

### Sandbox Management

Sandboxes are created under `/var/lib/vorpal/sandbox/` with UUID v7 names
(`cli/src/command/store/paths.rs:152-153`). They are cleaned up after each build but use
synchronous `remove_dir_all` which can be slow for large build workspaces.

---

## 7. gRPC and Networking

### Connection Management

- **Lazy connections**: UDS connections use `connect_with_connector_lazy` to defer actual
  connection until the first RPC call (`sdk/rust/src/context.rs:536-545`).
- **Eager connections**: TCP/TLS connections use `endpoint.connect().await` which establishes the
  connection immediately (`sdk/rust/src/context.rs:566-569`).
- No connection pooling, keepalive, or retry configuration is set on gRPC channels.
- Each service client creates a new channel -- there is no channel reuse between the agent,
  archive, artifact, and worker service clients created during a build.

### Auth Interceptor Blocking

The OIDC auth interceptor uses `tokio::task::block_in_place` with `Handle::current().block_on`
for async JWT validation (`cli/src/command/start/auth.rs:268-272`). This blocks a Tokio worker
thread during token validation, which under high concurrency could exhaust the thread pool.
The code includes a comment acknowledging this: "For high-throughput, prefer a tower layer that
supports async."

### Config Server Connection Retry

When the CLI starts a config binary as a subprocess, it retries the gRPC connection up to 3
times with a 500ms delay (`cli/src/command/config.rs:491-520`). This fixed retry strategy means:
- Maximum wait: 1.5 seconds before giving up.
- No exponential backoff.
- No jitter.

---

## 8. TypeScript Build Pipeline

The TypeScript language builder (`sdk/rust/src/artifact/language/typescript.rs`) generates a
build script that:

1. Runs `bun install --frozen-lockfile` (installs dependencies).
2. Runs `bun build --compile {entrypoint} --outfile $VORPAL_OUTPUT/bin/{name}`.

**Performance characteristics:**
- Bun is used as the TypeScript runtime and bundler, chosen for its fast startup and build times
  compared to Node.js/tsc.
- `bun build --compile` produces a single self-contained binary, which eliminates
  `node_modules` from the artifact output.
- The Bun binary itself is an artifact dependency, downloaded and cached through the normal
  artifact system.
- `--frozen-lockfile` ensures reproducibility but requires `bun.lockb` to exist.

---

## 9. Known Bottlenecks and Gaps

### Critical

1. **Sequential artifact builds**: Independent artifacts are built one at a time even when the
   DAG allows parallelism. This is the single largest performance bottleneck for projects with
   many independent artifacts.

2. **Full in-memory archive buffering**: Archives are fully read into memory during transfer.
   For artifacts larger than available RAM, builds will fail with out-of-memory errors.

3. **Sequential source hashing**: Source file digests are computed one file at a time. For
   projects with many source files, parallel hashing would improve throughput.

### Moderate

4. **Inconsistent chunk sizes**: Agent and worker use 8 KB chunks while registry uses 2 MB.
   The 8 KB size creates excessive per-message overhead for large transfers.

5. **No gRPC channel reuse**: Multiple service clients each create their own channel during
   a build. Connection setup overhead is paid multiple times.

6. **Auth interceptor thread blocking**: `block_in_place` during JWT validation can exhaust
   Tokio worker threads under high concurrency.

7. **Credential file read per request**: OAuth2 tokens are read from disk on every
   authenticated request instead of being cached in memory.

8. **S3 single-part upload**: No multipart upload support means S3 uploads are limited to 5 GB
   and don't benefit from parallel upload throughput.

### Minor

9. **Unbounded Moka cache**: The archive check cache has no maximum entry count, so it can
   grow without bound during the TTL window.

10. **No JWKS periodic refresh**: JWKS is only refreshed on cache miss (unknown `kid`). If all
    keys rotate simultaneously and the old key is still used for in-flight tokens, validation
    failures can occur until the cache refreshes.

11. **HashMap cloning**: The artifact store is cloned in several places where a reference or
    `Arc` would suffice.

---

## 10. Benchmarking and Profiling

### Current State

**There are no benchmarks in the codebase.** The project does not include:
- No Criterion benchmarks.
- No flamegraph tooling or profiling scripts.
- No load testing infrastructure.
- No build time tracking or performance regression tests.

The `Cargo.toml` files do not include `[bench]` sections or benchmark dependencies.

### Build Profiling

The CLI supports `--level debug` and `--level trace` log levels via `tracing-subscriber`,
which can be used for manual timing analysis by examining log timestamps. However, there are
no structured performance metrics, no span-based timing, and no integration with tracing-based
flamegraph tools like `tracing-flame`.

---

## 11. Scaling Considerations

### Single-Server Architecture

The current architecture runs all services (agent, registry, worker) as gRPC services within a
single process. The `--services` flag controls which services are enabled on a given instance,
but there is no built-in service discovery, load balancing, or horizontal scaling mechanism.

### Storage Scaling

- **Local backend**: Scales with the underlying filesystem. No cleanup/eviction policy exists
  beyond manual `vorpal system prune`.
- **S3 backend**: Scales inherently with S3 but is limited by single-part upload size and the
  lack of transfer acceleration or parallel upload.

### Connection Scaling

- **UDS mode** (default): Single Unix socket, limited to local connections. No concurrent listener
  configuration.
- **TCP mode**: Binds to `[::]:{port}`, limited by the OS's TCP connection limits and Tokio's
  default task scheduler.
- **TLS mode**: Uses `ring` for crypto, which is performant but adds per-connection TLS
  handshake overhead.

### Artifact Graph Scaling

For very large dependency graphs:
- `petgraph` topological sort is O(V + E), efficient.
- But sequential building means wall-clock time is O(sum of all build times) rather than
  O(critical path length).
- Each artifact's JSON serialization is hashed for digest computation. For deeply nested
  artifacts with many sources and steps, serialization overhead grows.
