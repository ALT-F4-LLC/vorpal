---
title: Artifacts
description: How Vorpal's content-addressed artifact model ensures reproducible builds.
---

An artifact is the core unit of work in Vorpal. It represents something you want to build -- a binary, a library, a development environment, or any other output. Artifacts are content-addressed, meaning they are identified by a SHA-256 hash of their inputs. This page explains how artifacts work and why they are designed this way.

## Anatomy of an artifact

Every artifact has four components:

- **Name** -- A human-readable identifier (e.g., `my-app`, `dev-shell`)
- **Sources** -- Input files that the build steps operate on. Sources can come from the local filesystem or HTTP URLs.
- **Steps** -- The build instructions. Each step has an entrypoint (the program that runs the step), arguments, environment variables, and optional secrets.
- **Target systems** -- The platforms this artifact can be built for (e.g., `AARCH64_DARWIN` for macOS Apple Silicon, `X8664_LINUX` for Linux x86_64)

```
Artifact
  |-- name: "my-app"
  |-- systems: [AARCH64_DARWIN, X8664_LINUX, ...]
  |-- sources:
  |     |-- name, path, digest, includes, excludes
  |-- steps:
        |-- entrypoint (e.g., "bash", "docker")
        |-- script or arguments
        |-- dependency artifacts (by digest)
        |-- environment variables
        |-- secrets (encrypted)
```

## Content addressing

Vorpal serializes the entire artifact definition (name, sources, steps, target systems) to JSON and computes a SHA-256 hash. This hash -- the **content digest** -- becomes the artifact's identity.

This design has two important properties:

1. **Deterministic caching** -- Two artifacts with identical inputs always produce the same digest. If you have not changed anything, the cached output is guaranteed to be correct. There is no cache invalidation logic to get wrong.

2. **Safe sharing** -- Because the digest encodes all inputs, you can safely share cached artifacts across machines and teams. If two developers build the same artifact with the same inputs, they get the same digest and can reuse each other's cached output.

When you change any input -- a source file, an environment variable, a build step argument -- the digest changes, and Vorpal treats it as a new artifact that needs to be built from scratch. This is intentional: it eliminates an entire class of "stale cache" bugs.

## Sources

Sources define the input files for your build. Each source has:

- A **path** -- local filesystem path or HTTP URL
- **Includes/excludes** -- file patterns to filter which files are included
- A **digest** -- SHA-256 hash of the source content, computed automatically

For local sources, Vorpal reads files from disk and computes their content hash. For HTTP sources, Vorpal downloads the archive, auto-detects its format (gzip, bzip2, xz, zip), unpacks it, and computes the hash of the contents.

### Lockfile

Source digests are recorded in `Vorpal.lock`. Once a source is locked, Vorpal rejects changes to that source unless you explicitly pass `--unlock`. This prevents builds from silently changing because an upstream URL started serving different content.

```json
{
  "lockfile": 1,
  "sources": [
    {
      "name": "toolchain",
      "digest": "a1b2c3...",
      "platform": "aarch64-darwin",
      "path": "https://example.com/toolchain.tar.gz"
    }
  ]
}
```

The lockfile pins digests per-platform because the same source URL may serve different binaries for different architectures.

## Build steps

Each artifact has one or more build steps that are executed sequentially. A step consists of:

- **Entrypoint** -- The program that executes the step. Defaults to `bash`, but can be `docker`, `bwrap` (Bubblewrap), or any executable.
- **Script or arguments** -- Either an inline script (for shell entrypoints) or command-line arguments (for other entrypoints).
- **Dependency artifacts** -- References to other artifacts whose outputs this step needs. Dependencies are available as directories at paths provided through environment variables.
- **Environment variables** -- Key-value pairs injected into the step's environment.
- **Secrets** -- Sensitive values that are encrypted by the Agent and decrypted by the Worker at execution time. Never stored in plaintext in the artifact definition.

During execution, each step receives these environment variables:

| Variable | Description |
|----------|-------------|
| `VORPAL_OUTPUT` | Path where the step should write its output |
| `VORPAL_WORKSPACE` | Path to the working directory containing sources |
| `VORPAL_ARTIFACT_<digest>` | Path to each dependency artifact's output |

## Artifact types

The SDKs provide high-level builders for common artifact patterns:

### Language artifacts

`Rust`, `Go`, and `TypeScript` builders handle the full compilation pipeline for their respective languages. You specify source files, and the builder generates the appropriate build steps, toolchain dependencies, and platform targeting.

### Development environments

`DevelopmentEnvironment` creates portable development shells with pinned tools and environment variables. When activated, a development environment provides a consistent set of tools regardless of what is installed on the host system.

### User environments

`UserEnvironment` installs tools and configurations into the user's home directory (`~/.vorpal/`). Unlike development environments (which are project-scoped), user environments persist across projects.

### Custom artifacts

The base `Artifact` type lets you define arbitrary build steps with any entrypoint. This is how you use Docker, Bubblewrap, or custom executors. See the [Quickstart](../getting-started/quickstart) for a basic example, or the SDK guides ([Rust](../guides/rust), [Go](../guides/go), [TypeScript](../guides/typescript)) for detailed usage.

## Cross-platform targeting

Every artifact declares which platforms it supports using the `ArtifactSystem` enum:

| Value | Platform |
|-------|----------|
| `AARCH64_DARWIN` | macOS Apple Silicon |
| `AARCH64_LINUX` | Linux ARM64 |
| `X8664_DARWIN` | macOS Intel |
| `X8664_LINUX` | Linux x86_64 |

When you build an artifact, Vorpal only builds it for the current host platform. The target systems declaration is used by the SDK builders to generate platform-appropriate build steps (e.g., selecting the right toolchain download URL for the host architecture).

## Aliases

Artifacts can have named aliases like `latest` or a version string. Aliases let you run artifacts by name (`vorpal run my-app`) instead of by their full content digest. The Registry stores the mapping from alias to digest.
