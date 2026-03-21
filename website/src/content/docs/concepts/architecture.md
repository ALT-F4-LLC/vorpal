---
title: Architecture
description: How Vorpal's client-server architecture orchestrates builds across services.
---

Vorpal uses a client-server architecture where your build configuration is a real program that communicates with backend services over gRPC. This page explains how the pieces fit together and why the system is designed this way.

## Three layers

Vorpal consists of three layers that work together to turn your build configuration into reproducible artifacts:

```
                      +-------------------+
                      |   Your Config     |
                      | (Rust/Go/TS code) |
                      +--------+----------+
                               |
                          SDK (language)
                               |
                      +--------v----------+
                      |    vorpal CLI     |
                      +---+-----+-----+---+
                          |     |     |
                +---------+  +--+--+  +---------+
                |            |     |            |
           +----v----+ +----v----+ +----v------+
           |  Agent  | | Worker  | | Registry  |
           | Service | | Service | | Service   |
           +---------+ +---------+ +-----------+
```

**SDK** -- You write your build configuration as a program using the Vorpal SDK in Rust, Go, or TypeScript. The SDK provides builder types for defining artifacts, sources, build steps, and target platforms. Your config is compiled and executed like any other program.

**CLI** -- The `vorpal` command-line tool is the single entry point for all interactions. It compiles your build configuration, orchestrates the build process, and manages system services. When you run `vorpal build`, the CLI compiles your config, starts it as a subprocess, and queries it for artifact definitions over gRPC.

**Services** -- Three backend services handle the actual work. They run together in a single `vorpal` process started by `vorpal system services start`, communicating over gRPC.

## Services in detail

### Agent

The Agent prepares artifacts before they are built. When the CLI submits an artifact for building, the Agent:

1. Resolves sources -- fetches content from the local filesystem or HTTP URLs
2. Computes content digests (SHA-256) for each source, creating the content-addressed identity
3. Checks the lockfile (`Vorpal.lock`) to pin source digests and detect unexpected changes
4. Encrypts any secrets attached to build steps
5. Pushes prepared source archives to the Registry

The Agent acts as a gatekeeper: it ensures that all inputs are accounted for, hashed, and locked before any build work begins.

### Worker

The Worker executes build steps. For each artifact that needs building, it:

1. Pulls source archives and dependency artifacts from the Registry
2. Creates an isolated workspace directory
3. Runs each build step as a subprocess with controlled environment variables (`VORPAL_OUTPUT`, `VORPAL_WORKSPACE`, dependency paths)
4. Compresses the build output with zstd and pushes it to the Registry
5. Registers the completed artifact with its content digest

Build steps run the entrypoint you specify -- by default a Bash script, but you can use Docker, Bubblewrap, or any executable as the entrypoint.

### Registry

The Registry provides storage for both binary archives and artifact metadata. It has two sub-services:

- **Archive Service** -- stores and retrieves binary blobs (source archives, built artifact archives). Supports local filesystem or S3 backends.
- **Artifact Service** -- stores artifact metadata and manages aliases. Aliases let you refer to artifacts by name (e.g., `vorpal run my-app`) rather than by content digest.

## Build flow

When you run `vorpal build <name>`, the following sequence occurs:

1. **Configuration resolution** -- The CLI merges settings from CLI flags, `Vorpal.toml` (project config), `~/.vorpal/settings.json` (user config), and built-in defaults.

2. **Configuration compilation** -- The CLI compiles your build configuration program, starts it as a subprocess, and queries it for artifact definitions via gRPC.

3. **Dependency ordering** -- Artifacts are topologically sorted based on their dependency references. If artifact B depends on artifact A, A is always built first.

4. **For each artifact (in dependency order):**
   - Check the local cache -- if the artifact output already exists, skip it entirely
   - Try pulling from the Registry -- if a remote cache hit exists, download it
   - Build via the Worker -- if not cached anywhere, prepare sources through the Agent, then build through the Worker

5. **Output** -- The CLI prints the artifact digest. Use `--path` to get the filesystem path to the output.

This flow means that on a fresh build, everything is built from scratch. On subsequent builds, only artifacts whose inputs have changed are rebuilt -- everything else is served from the content-addressed cache. See [Caching](./caching) for details on how this works.

## Communication

All inter-component communication uses gRPC with Protocol Buffers. This provides:

- **Strongly-typed contracts** across all three SDK languages -- the same `.proto` definitions generate client code for Rust, Go, and TypeScript
- **Streaming** for transferring large archives without loading them entirely into memory
- **TLS support** for securing communication when running services remotely

By default, services communicate over a Unix domain socket at `/var/lib/vorpal/vorpal.sock`. For remote deployments, TCP with optional TLS is available. See the [Installation](../getting-started/installation) page for details on TLS key generation.

## Why config-as-code?

Traditional build systems use YAML, JSON, or custom DSLs for configuration. Vorpal takes a different approach: your build configuration is a real program.

This means you get full IDE support (autocompletion, type checking, go-to-definition), the ability to use conditionals and loops, and the option to share configuration as libraries. There is no special syntax to learn -- if you know how to write code in your language, you know how to write a Vorpal build configuration.

The tradeoff is that Vorpal must compile and execute your configuration program before it can start building. In practice, this adds a few seconds to the build startup, but the benefits of type safety and IDE support outweigh this cost for most projects.

## Why a single process?

The Agent, Worker, and Registry run in a single OS process rather than as separate microservices. This simplifies deployment and local development -- you start one process and everything works. The services still maintain clean boundaries through their gRPC interfaces, so they could be separated in the future if needed.

You can selectively enable services using the `--services` flag (e.g., `--services agent,worker` to run without a local registry).
