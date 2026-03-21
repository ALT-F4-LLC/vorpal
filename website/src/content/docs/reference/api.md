---
title: API Reference
description: Vorpal gRPC/Protobuf API reference for SDK authors and service integrators.
---

Vorpal services communicate via gRPC using Protocol Buffers. This reference documents the five service definitions that compose the Vorpal API. SDK authors use these definitions to generate client and server code in Rust, Go, and TypeScript.

Proto source files are located at `sdk/rust/api/` in the repository.

## Code Generation

| Language | Tool | Output |
|----------|------|--------|
| Rust | `tonic-prost-build` via `sdk/rust/build.rs` | Inline (generated at build time) |
| Go | `protoc` + `protoc-gen-go` / `protoc-gen-go-grpc` | `sdk/go/pkg/api/` |
| TypeScript | `protoc-gen-ts_proto` | `sdk/typescript/src/api/` |

Regenerate all generated code with `make generate`.

## Common Types

### `ArtifactSystem`

Enum representing supported target platforms.

```protobuf
enum ArtifactSystem {
    UNKNOWN_SYSTEM = 0;
    AARCH64_DARWIN = 1;
    AARCH64_LINUX = 2;
    X8664_DARWIN = 3;
    X8664_LINUX = 4;
}
```

### `Artifact`

The core build unit. Defines what to build, from what sources, using what steps, and for which platforms.

```protobuf
message Artifact {
    ArtifactSystem target = 1;       // Host platform for this build
    repeated ArtifactSource sources = 2;
    repeated ArtifactStep steps = 3;
    repeated ArtifactSystem systems = 4;  // Platforms this artifact supports
    repeated string aliases = 5;     // Named references (e.g., "latest")
    string name = 6;
}
```

### `ArtifactSource`

Source input for an artifact. Can reference local paths, HTTP URLs, or git repositories.

```protobuf
message ArtifactSource {
    optional string digest = 1;      // SHA-256 content digest (set after resolution)
    repeated string excludes = 2;    // Glob patterns to exclude
    repeated string includes = 3;    // Glob patterns to include
    string name = 4;                 // Source identifier
    string path = 5;                 // Local path, HTTP URL, or git URL
}
```

### `ArtifactStep`

An individual build step with its execution configuration.

```protobuf
message ArtifactStep {
    optional string entrypoint = 1;            // Executable to run
    optional string script = 2;                // Inline script content
    repeated ArtifactStepSecret secrets = 3;   // Encrypted secrets
    repeated string arguments = 4;             // Command arguments
    repeated string artifacts = 5;             // Dependency artifact digests
    repeated string environments = 6;          // Environment variables (KEY=VALUE)
}
```

### `ArtifactStepSecret`

Encrypted secret passed to a build step.

```protobuf
message ArtifactStepSecret {
    string name = 1;    // Secret name (exposed as env var)
    string value = 2;   // Encrypted value
}
```

## AgentService

**Package:** `vorpal.agent`

The agent prepares artifacts before building. It handles source resolution, content hashing, lockfile management, and secret encryption.

```protobuf
service AgentService {
    rpc PrepareArtifact(PrepareArtifactRequest)
        returns (stream PrepareArtifactResponse);
}
```

### `PrepareArtifact`

Resolves sources, computes content digests, and prepares the artifact for the worker. Returns a stream of progress updates and the final prepared artifact.

**Request:**

```protobuf
message PrepareArtifactRequest {
    bool artifact_unlock = 1;          // Allow source digest changes
    string artifact_context = 2;       // Build context directory path
    string artifact_namespace = 3;     // Artifact namespace
    string registry = 4;              // Registry address
    vorpal.artifact.Artifact artifact = 5;
}
```

**Response (streamed):**

```protobuf
message PrepareArtifactResponse {
    optional string artifact_digest = 1;   // Content digest (final message)
    optional string artifact_output = 2;   // Progress output text
    vorpal.artifact.Artifact artifact = 3; // Prepared artifact (final message)
}
```

## ArchiveService

**Package:** `vorpal.archive`

Binary blob storage for source and artifact archives. Supports local filesystem and S3 backends.

```protobuf
service ArchiveService {
    rpc Check(ArchivePullRequest) returns (ArchiveResponse);
    rpc Pull(ArchivePullRequest) returns (stream ArchivePullResponse);
    rpc Push(stream ArchivePushRequest) returns (ArchiveResponse);
}
```

### `Check`

Check whether an archive exists in the registry.

### `Pull`

Download an archive as a stream of byte chunks.

### `Push`

Upload an archive as a stream of byte chunks.

**Request/Response types:**

```protobuf
message ArchivePullRequest {
    string digest = 1;       // Archive content digest
    string namespace = 2;    // Artifact namespace
}

message ArchivePushRequest {
    bytes data = 1;          // Chunk data
    string digest = 2;       // Archive content digest
    string namespace = 3;    // Artifact namespace
}

message ArchiveResponse {}

message ArchivePullResponse {
    bytes data = 1;          // Chunk data
}
```

## ArtifactService

**Package:** `vorpal.artifact`

Artifact metadata storage and retrieval. Manages artifact definitions, aliases, and digest lookups.

```protobuf
service ArtifactService {
    rpc GetArtifact(ArtifactRequest) returns (Artifact);
    rpc GetArtifactAlias(GetArtifactAliasRequest)
        returns (GetArtifactAliasResponse);
    rpc GetArtifacts(ArtifactsRequest) returns (ArtifactsResponse);
    rpc StoreArtifact(StoreArtifactRequest) returns (ArtifactResponse);
}
```

### `GetArtifact`

Retrieve an artifact definition by its content digest.

```protobuf
message ArtifactRequest {
    string digest = 1;
    string namespace = 2;
}
```

### `GetArtifactAlias`

Resolve a named alias to a digest. Used by `vorpal run` to find artifacts by name.

```protobuf
message GetArtifactAliasRequest {
    ArtifactSystem system = 1;
    string name = 2;
    string namespace = 3;
    string tag = 4;
}

message GetArtifactAliasResponse {
    string digest = 1;
}
```

### `GetArtifacts`

List all artifact digests in a namespace.

```protobuf
message ArtifactsRequest {
    repeated string digests = 1;
    string namespace = 2;
}

message ArtifactsResponse {
    repeated string digests = 1;
}
```

### `StoreArtifact`

Store an artifact definition with optional aliases.

```protobuf
message StoreArtifactRequest {
    Artifact artifact = 1;
    repeated string artifact_aliases = 2;
    string artifact_namespace = 3;
}

message ArtifactResponse {
    string digest = 1;
}
```

## ContextService

**Package:** `vorpal.context`

Artifact retrieval from a running configuration binary. The CLI starts the compiled config program as a subprocess, which exposes this service to provide artifact definitions.

```protobuf
service ContextService {
    rpc GetArtifact(vorpal.artifact.ArtifactRequest)
        returns (vorpal.artifact.Artifact);
    rpc GetArtifacts(vorpal.artifact.ArtifactsRequest)
        returns (vorpal.artifact.ArtifactsResponse);
}
```

This service reuses the `ArtifactRequest`, `ArtifactsRequest`, and `ArtifactsResponse` types from the `vorpal.artifact` package.

## WorkerService

**Package:** `vorpal.worker`

Executes artifact build steps. The worker pulls sources and dependencies from the registry, runs build steps as subprocesses, and pushes outputs back to the registry.

```protobuf
service WorkerService {
    rpc BuildArtifact(BuildArtifactRequest)
        returns (stream BuildArtifactResponse);
}
```

### `BuildArtifact`

Build an artifact. Returns a stream of build output lines.

**Request:**

```protobuf
message BuildArtifactRequest {
    repeated string artifact_aliases = 1;
    string artifact_namespace = 2;
    vorpal.artifact.Artifact artifact = 3;
    string registry = 4;
}
```

**Response (streamed):**

```protobuf
message BuildArtifactResponse {
    string output = 1;    // Build output line
}
```

## Build Step Environment

When a worker executes a build step, these environment variables are available:

| Variable | Description |
|----------|-------------|
| `VORPAL_OUTPUT` | Path to the artifact output directory |
| `VORPAL_WORKSPACE` | Path to the build workspace directory |
| `VORPAL_ARTIFACT_<DIGEST>` | Path to each dependency artifact's output |

Additionally, all entries in the step's `environments` list are set as environment variables, and all `secrets` are decrypted and set as environment variables by name.

## Authentication

When an OIDC issuer is configured on the server, gRPC interceptors validate JWT tokens on registry and worker endpoints. Clients include the token in the `authorization` metadata header.

- **CLI to services:** Uses tokens obtained via `vorpal login` (device flow)
- **Worker to registry:** Uses OAuth2 client credentials flow
- **Namespace authorization:** JWT claims can restrict access to specific namespaces
