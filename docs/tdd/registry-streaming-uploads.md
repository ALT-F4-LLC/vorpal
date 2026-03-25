---
project: vorpal
maturity: draft
last_updated: 2026-03-25
updated_by: "@staff-engineer"
scope: "Eliminate full-memory buffering of archives during registry push to prevent OOMKilled under Kubernetes resource limits"
owner: "@staff-engineer"
dependencies:
  - ../spec/architecture.md
  - ../spec/performance.md
---

# Technical Design Document: Registry Streaming Uploads

## 1. Problem Statement

The Vorpal registry service buffers entire archive uploads in memory before writing them to the storage backend. When running in Kubernetes with typical container memory limits (256Mi-512Mi), uploading large archives (e.g., toolchain builds, monorepo source bundles) causes the container to be OOMKilled:

```
The container last terminated 4 minutes ago with exit code 137 because of OOMKilled.
```

The root cause is in `cli/src/command/start/registry.rs:239-281`, where the `ArchiveService::push` implementation collects the entire client-streaming request into a `Vec<u8>` before passing it to the backend:

```rust
async fn push(
    &self,
    request: Request<Streaming<ArchivePushRequest>>,
) -> Result<Response<ArchiveResponse>, Status> {
    let mut request_data: Vec<u8> = vec![];       // <-- unbounded allocation
    // ...
    while let Some(request) = request_stream.next().await {
        let request = request.map_err(|err| Status::internal(err.to_string()))?;
        request_data.extend_from_slice(&request.data); // <-- grows with archive size
        // ...
    }
    // ...
    let request = ArchivePushRequest {
        digest: request_digest,
        data: request_data,                         // <-- entire archive in memory
        namespace: request_namespace,
    };
    self.backend.push(&request).await?;             // <-- backend receives full blob
```

Additionally, both sending clients (agent at `agent.rs:379-391` and worker at `worker.rs:868-880`) read the entire archive file into memory before chunking:

```rust
// agent.rs:379
let source_archive_data = read(&source_sandbox_archive).await?;
let mut source_stream = vec![];
for chunk in source_archive_data.chunks(DEFAULT_CHUNKS_SIZE) { ... }

// worker.rs:868
let artifact_data = read(&artifact_archive).await?;
let mut request_stream = vec![];
for chunk in artifact_data.chunks(DEFAULT_CHUNKS_SIZE) { ... }
```

### Why Now

This is a production blocker. The registry cannot reliably operate in Kubernetes without either (a) grossly overprovisioning memory limits or (b) restricting archive sizes. Neither is sustainable as the project grows and artifacts become larger.

### Constraints

- **No protocol changes**: The gRPC `Push(stream ArchivePushRequest) -> ArchiveResponse` contract is already a client-streaming RPC. The protobuf messages remain unchanged.
- **No persistence changes**: Storage paths, archive format (tar.zst), and backend abstraction are unchanged.
- **No K8s resource limit changes**: The fix must be in the application code, not in infrastructure.
- **Backward compatible**: Existing clients continue to work without modification. The server improvement is transparent to callers.

### Acceptance Criteria

| # | Criterion | Verification |
|---|---|---|
| AC-1 | Registry peak memory during a 500MB archive upload stays below 50MB above baseline | Measure with a test harness: upload 500MB archive, observe RSS via `/proc/self/status` or `jemalloc` stats |
| AC-2 | Registry push succeeds for archives larger than the container memory limit | Upload a 1GB archive to a registry with 512Mi limit; no OOMKill |
| AC-3 | Both local and S3 backends pass all existing tests | `cargo test` for the registry module |
| AC-4 | No protobuf schema changes | Diff `sdk/rust/api/archive/archive.proto` shows zero changes |
| AC-5 | Agent and worker upload archives without reading entire file into memory | Code review: no `read(&path).await` followed by full-file `Vec<u8>` for archive uploads |
| AC-6 | Archive push with digest mismatch is rejected (integrity preserved) | Test: push archive with wrong digest, verify error response |
| AC-7 | Concurrent uploads do not interfere with each other | Test: 3 simultaneous pushes of different archives, all succeed |

## 2. Context & Prior Art

### Current Architecture

The registry's `ArchiveBackend` trait defines push as:

```rust
async fn push(&self, req: &ArchivePushRequest) -> Result<(), Status>;
```

This signature expects the full `ArchivePushRequest` (which includes `data: Vec<u8>` containing the entire archive). Both `LocalBackend` and `S3Backend` implementations consume the full blob:

- **LocalBackend** (`archive/local.rs:50-68`): Calls `tokio::fs::write(&path, &request.data)` -- single write of the full blob.
- **S3Backend** (`archive/s3.rs:65-96`): Calls `client.put_object().body(request.data.clone().into())` -- single PutObject with the full blob.

### How Other Systems Solve This

- **Docker Registry v2**: Uses chunked uploads with a PATCH-based protocol. Each chunk is written to a temporary file, then committed atomically via a final PUT with the digest.
- **AWS S3**: Provides multipart upload for objects > 5MB. Parts are uploaded independently and assembled server-side.
- **OCI Distribution Spec**: Supports both monolithic and chunked blob uploads with session-based upload IDs.
- **tonic/gRPC**: Client-streaming RPCs naturally deliver messages as a stream via `Streaming<T>`. The framework does not force buffering -- the current code does so by choice.

The key insight is that tonic already delivers `ArchivePushRequest` chunks as a stream. The buffering is entirely in our application code, not a framework limitation.

### Existing Streaming Precedent in Codebase

The **pull** path already streams correctly:
- `S3Backend::pull` (`archive/s3.rs:25-63`): Reads from S3 via `get_object().body` stream and forwards chunks through the `mpsc::Sender` without full buffering.
- `LocalBackend::pull` (`archive/local.rs:24-48`): Reads the file and sends 2MB chunks (though it does read the full file first with `tokio::fs::read`).

This demonstrates the team is already comfortable with streaming patterns. The push path simply needs the same treatment.

## 3. Alternatives Considered

### Alternative A: Stream-to-Temp-File then Rename (Recommended)

Write incoming chunks to a temporary file as they arrive. After the stream completes, rename the temp file to the final path (local) or complete the upload (S3 multipart).

**Strengths:**
- Memory usage bounded by chunk size (8KB currently) + small buffers
- Atomic: partial uploads don't pollute the store (temp file is cleaned up on failure)
- Simple: leverages filesystem for buffering, no new data structures
- Works identically for local and S3 backends (temp file vs. multipart)

**Weaknesses:**
- Requires temporary disk space equal to the archive size (acceptable -- disk is cheap, memory is not)
- S3 multipart upload adds API complexity (CreateMultipartUpload, UploadPart, CompleteMultipartUpload)

### Alternative B: In-Memory Ring Buffer with Backpressure

Use a bounded in-memory buffer (e.g., 32MB) with backpressure signaling to the gRPC stream.

**Strengths:**
- No temporary files on disk
- Potentially lower latency for small archives

**Weaknesses:**
- Still requires significant memory per concurrent upload (32MB x N connections)
- Complex backpressure logic across gRPC and backend write paths
- Does not solve the fundamental problem -- just raises the threshold
- S3 PutObject still requires the full body or multipart; ring buffer does not help

### Alternative C: Raise Kubernetes Memory Limits

Simply increase memory limits to accommodate large archives.

**Strengths:**
- Zero code changes

**Weaknesses:**
- Does not fix the root cause -- archives can always grow larger than any limit
- Wastes cluster resources
- Explicitly out of scope per problem statement

### Recommendation

**Alternative A (stream-to-temp-file)** is the clear choice. It bounds memory usage regardless of archive size, uses the simplest mechanism (filesystem), and has precedent in Docker Registry and OCI spec. The temp file approach also provides natural atomicity -- the final artifact only appears when the upload is complete.

## 4. Architecture & System Design

### 4.1 ArchiveBackend Trait Change

The `ArchiveBackend::push` signature changes to accept a stream of chunks instead of a complete request:

```rust
#[tonic::async_trait]
pub trait ArchiveBackend: Send + Sync + 'static {
    // ... existing methods unchanged ...

    async fn push(
        &self,
        digest: &str,
        namespace: &str,
        stream: &mut (dyn Stream<Item = Result<bytes::Bytes, Status>> + Unpin + Send),
    ) -> Result<(), Status>;
}
```

This changes the push signature from receiving a materialized `ArchivePushRequest` to receiving metadata (digest, namespace) plus a stream of raw byte chunks. The `ArchiveService::push` gRPC handler adapts the `Streaming<ArchivePushRequest>` into this interface.

### 4.2 ArchiveService::push Handler (registry.rs)

The handler extracts metadata (digest, namespace) from the first chunk and then passes the stream through to the backend:

```
1. Receive first chunk from stream
2. Extract digest + namespace from first chunk
3. Validate digest and namespace are non-empty
4. Create an adapter stream that yields just the `data` bytes from remaining chunks
5. Call backend.push(digest, namespace, &mut adapted_stream)
6. Return ArchiveResponse on success
```

Key property: **no `Vec<u8>` accumulation**. Each chunk's data bytes flow from the gRPC transport through to the backend writer and are then dropped.

### 4.3 LocalBackend Streaming Push

```
1. Create temp file in the same parent directory as the final path
   (same filesystem ensures rename is atomic)
2. Open temp file with BufWriter
3. Read chunks from stream, write each to BufWriter
4. Flush and sync BufWriter
5. Rename temp file to final path
6. On error: remove temp file
```

Using `tokio::io::BufWriter` wrapping `tokio::fs::File` provides async, buffered writes. The 8KB default BufWriter capacity aligns well with the 8KB incoming chunk size.

### 4.4 S3Backend Streaming Push

```
1. Check if archive already exists (head_object) -- short-circuit if exists
2. Initiate S3 multipart upload (create_multipart_upload)
3. Buffer incoming chunks until part buffer reaches 5MB (S3 minimum part size)
4. Upload each part (upload_part), collecting ETags
5. After stream ends, upload any remaining buffered data as final part
6. Complete multipart upload (complete_multipart_upload) with ETags
7. On error: abort multipart upload (abort_multipart_upload)
```

The 5MB part buffer is the minimum required by S3. This means peak memory per concurrent S3 upload is approximately 5MB + chunk overhead -- well within typical container limits even under high concurrency.

For archives smaller than 5MB, all data fits in a single part. The implementation can either use a single PutObject (simpler) or a single-part multipart upload (consistent code path). Single PutObject is preferred for simplicity.

### 4.5 Client-Side Streaming (Agent and Worker)

The agent (`agent.rs:379-404`) and worker (`worker.rs:868-890`) currently read entire archive files into memory before chunking. This should be changed to read-and-stream:

```
1. Open archive file
2. Create async stream that reads file in chunks (BufReader + read_buf)
3. Map each chunk into ArchivePushRequest with digest and namespace
4. Pass stream to gRPC client push()
```

This uses `tokio::fs::File` with `tokio::io::BufReader` and `tokio_stream::wrappers::ReceiverStream` or a manual `Stream` implementation to avoid materializing the full file.

### 4.6 Component Interaction Diagram

```
Agent/Worker                  Registry Server                 Storage Backend
     |                              |                              |
     |-- open file ----------------->                              |
     |-- read 8KB chunk ----------->|                              |
     |   ArchivePushRequest{        |                              |
     |     data: [8KB],             |                              |
     |     digest: "sha256:...",    |-- extract metadata --------->|
     |     namespace: "default"     |-- create temp/multipart ---->|
     |   }                          |                              |
     |-- read 8KB chunk ----------->|                              |
     |   ArchivePushRequest{        |-- write chunk to            |
     |     data: [8KB], ...         |   temp file / upload part -->|
     |   }                          |                              |
     |-- ... (repeat) ------------->|-- ... (write each) -------->|
     |                              |                              |
     |-- stream ends -------------->|-- finalize (rename /        |
     |                              |   complete multipart) ------>|
     |<-- ArchiveResponse ---------|<-- success ------------------|
```

## 5. Data Models & Storage

No changes to data models or storage layout. Archives remain at the same paths:
- Local: `/var/lib/vorpal/store/{namespace}/archive/{digest}.tar.zst`
- S3: `artifact/archive/{namespace}/{digest}.tar.zst`

Temporary files for local backend: same directory as final path, with a `.tmp` suffix or UUID-based name to avoid collisions.

## 6. API Contracts

No protobuf changes. The gRPC contract is already correct:

```protobuf
rpc Push(stream ArchivePushRequest) returns (ArchiveResponse);
```

The only change is internal: the Rust `ArchiveBackend` trait signature. This is a private API within the `cli` crate.

## 7. Migration & Rollout

### Migration Path

This is a drop-in improvement with no migration needed:
1. Deploy new registry binary
2. Existing clients (agents, workers) continue to work unchanged because the gRPC protocol is unchanged
3. Client-side streaming improvements (agent/worker) can be deployed independently

### Rollout Phases

1. **Server-side first**: Update registry push handler and both backends. This fixes the OOM regardless of client behavior.
2. **Client-side second**: Update agent and worker to stream from disk. This reduces client memory usage but is not critical for the OOM fix.

### Rollback

Revert to previous binary. No data migration, no schema changes, no state to clean up.

## 8. Risks & Open Questions

### Risks

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Temp file accumulation on crash | Medium | Low | Use temp dir cleanup on startup; temp files use `.tmp` suffix for easy identification |
| S3 multipart upload orphans on crash | Medium | Low | S3 lifecycle policy to abort incomplete multipart uploads after 24h (standard practice) |
| Increased disk I/O on local backend | Low | Low | Writes are sequential and buffered; modern SSDs handle this without issue |
| Partial write on network error | Medium | Low | Temp file is cleaned up; final rename only happens after full stream |

### Open Questions

1. **S3 part size**: Should we use exactly 5MB (minimum) or a larger part size (e.g., 8MB, 16MB)? Larger parts mean fewer API calls but more memory per upload. **Recommendation**: Start with 5MB; tune later based on production observation.
2. **Temp file location**: Should temp files go in the same directory as the final path (guarantees same-filesystem rename) or in a dedicated temp directory? **Recommendation**: Same directory, for atomic rename guarantees.
3. **Maximum archive size**: Should we enforce a server-side maximum archive size? Currently there is no limit. **Recommendation**: Defer -- this is orthogonal to the streaming fix and can be added later as a separate concern.

### Assumptions

- The existing 8KB client chunk size, while small, is functional. Optimizing chunk size (the performance spec notes the asymmetry with the 2MB pull chunk size) is a separate concern.
- Disk space on the registry node/pod is sufficient to hold temporary files during upload. This is already a requirement since archives are stored on disk anyway.

## 9. Testing Strategy

### Unit Tests

- **LocalBackend streaming push**: Mock a stream of byte chunks, verify file is written correctly, verify temp file is cleaned up on error, verify atomic rename behavior.
- **S3Backend streaming push**: Mock S3 client, verify multipart upload lifecycle (create, upload parts, complete), verify abort on error, verify small-archive path uses PutObject.
- **ArchiveService handler**: Verify metadata extraction from first chunk, verify empty-digest rejection, verify stream is forwarded to backend.

### Integration Tests

- **End-to-end push+pull**: Push an archive via streaming, pull it back, verify byte-for-byte equality.
- **Concurrent uploads**: Push 3 archives simultaneously, verify all succeed and are independently retrievable.
- **Large archive**: Push a 500MB archive to a registry container with 256Mi memory limit, verify no OOM.

### Regression Tests

- All existing `ArchiveServer` tests in `registry.rs` (cache tests) must pass unchanged.
- The `MockBackend` in tests needs to be updated for the new `push` signature.

## 10. Observability & Operational Readiness

### Key Metrics

- **Upload duration**: Log elapsed time per push at info level (already done: `info!("registry |> archive push: {}", request.digest)`)
- **Bytes written**: Log total bytes written after stream completion
- **Temp file cleanup**: Warn-level log if temp file cleanup fails

### Error Signals

- **Temp file write failure**: Indicates disk full or permissions issue -- log error with path and OS error
- **S3 multipart abort**: Log warning with upload ID for debugging
- **Stream error mid-upload**: Log the gRPC status code and number of bytes received so far

### 3am Diagnosability

If the registry starts OOMing again after this fix, check:
1. Are concurrent uploads consuming 5MB each? (Count active multipart uploads)
2. Is there a code path that accidentally re-introduced buffering?
3. Is the pull path (separate issue) buffering large downloads? (Out of scope for this TDD but noted in the performance spec: `build.rs:113` and `worker.rs:186` also buffer fully)

## 11. Implementation Phases

### Phase 1: Server-Side Streaming Push (Size: M)

**Files modified:**
- `cli/src/command/start/registry.rs` -- Change `ArchiveBackend::push` signature, rewrite `ArchiveService::push` handler
- `cli/src/command/start/registry/archive/local.rs` -- Implement streaming-to-temp-file push
- `cli/src/command/start/registry/archive/s3.rs` -- Implement S3 multipart upload push

**Dependencies:** None. This phase is self-contained.

**Key changes:**
1. Change `ArchiveBackend::push` to accept stream instead of materialized request
2. Update `ArchiveService::push` to pass chunks through without accumulating
3. `LocalBackend`: Write chunks to temp file, rename on completion
4. `S3Backend`: Use multipart upload API, buffer to 5MB part size
5. Update `MockBackend` in tests for new signature

### Phase 2: Client-Side Streaming Reads (Size: S)

**Files modified:**
- `cli/src/command/start/agent.rs` -- Stream file reads for source archive push
- `cli/src/command/start/worker.rs` -- Stream file reads for artifact archive push

**Dependencies:** None (independent of Phase 1; both can be done in parallel).

**Key changes:**
1. Replace `read(&path).await` + `chunks()` with `BufReader` + async stream
2. Use `tokio_stream` or `async_stream` to create a stream of `ArchivePushRequest` from file reads
3. Pass stream directly to gRPC client `push()`

### Phase 3: Tests and Verification (Size: S)

**Files modified:**
- `cli/src/command/start/registry.rs` (test module) -- Update mock, add streaming push tests

**Dependencies:** Phase 1 must be complete.

**Key changes:**
1. Update `MockBackend::push` to consume a stream
2. Add unit test for streaming push behavior
3. Add test verifying temp file cleanup on error
4. Verify all existing cache tests still pass
