---
title: Rust SDK
description: Build artifacts and environments with the Vorpal Rust SDK.
---

The Rust SDK lets you define Vorpal build configurations as native Rust programs. Your build config compiles to a binary that communicates with the Vorpal daemon over gRPC.

## Installation

Add the SDK to your project's `Cargo.toml`:

```toml title="Cargo.toml"
[dependencies]
vorpal-sdk = "0.1.0-alpha.0"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
```

## Project setup

Create a build configuration in `src/main.rs`:

```rust title="src/main.rs"
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    // Define your artifacts here

    ctx.run().await
}
```

Every Vorpal config starts by creating a context and defining target systems. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

### Build a Rust project

Use the `Rust` builder to compile a Rust project into a cross-platform artifact:

```rust title="src/main.rs" {3-4,12-16}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::language::rust::Rust,
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    Rust::new("my-app", systems)
        .with_bins(vec!["my-app"])
        .with_includes(vec!["src", "Cargo.lock", "Cargo.toml"])
        .build(ctx)
        .await?;

    ctx.run().await
}
```

The `Rust` builder:
- **`with_bins`** — Specifies which binaries to produce from the crate
- **`with_includes`** — Lists files and directories to include in the build source

### Development environments

Create a portable development shell with pinned tools and environment variables:

```rust title="src/main.rs" {3,11-13}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::DevelopmentEnvironment,
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    DevelopmentEnvironment::new("my-project", systems.clone())
        .with_environments(vec!["FOO=bar".into()])
        .build(ctx).await?;

    ctx.run().await
}
```

Activate the environment by sourcing the generated `bin/activate` script inside the artifact output.

### User environments

Install tools into your user-wide environment with symlinks:

```rust title="src/main.rs" {3,11-13}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::UserEnvironment,
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    UserEnvironment::new("my-home", systems)
        .with_symlinks(vec![("/path/to/local/bin/app", "$HOME/.vorpal/bin/app")])
        .build(ctx).await?;

    ctx.run().await
}
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```rust title="src/main.rs" {3-4,11-19}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{Artifact, ArtifactStep},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    let step = ArtifactStep::new("docker")
        .with_arguments(vec![
            "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
            "alpine", "sh", "-lc", "echo hi > /out/hi.txt",
        ])
        .build();

    Artifact::new("example-docker", vec![step], systems)
        .build(ctx).await?;

    ctx.run().await
}
```

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.

## Common patterns

### Builder options

The `Rust` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_bins(bins)` | Binary targets to produce |
| `with_includes(paths)` | Source files to include |
| `with_packages(pkgs)` | Workspace packages to build |
| `with_environments(vars)` | Environment variables for the build |
| `with_excludes(patterns)` | Files to exclude from source |
| `with_check(bool)` | Enable `cargo check` |
| `with_format(bool)` | Enable `cargo fmt --check` |
| `with_lint(bool)` | Enable `clippy` linting |
| `with_secrets(pairs)` | Build-time secrets |

### Multiple artifacts

Chain multiple artifacts in a single config — they share the same context and build graph:

```rust
Rust::new("lib-core", systems.clone())
    .with_includes(vec!["crates/core", "Cargo.lock", "Cargo.toml"])
    .build(ctx).await?;

Rust::new("bin-server", systems.clone())
    .with_bins(vec!["server"])
    .with_includes(vec!["crates/server", "Cargo.lock", "Cargo.toml"])
    .build(ctx).await?;

DevelopmentEnvironment::new("dev-shell", systems)
    .build(ctx).await?;
```
