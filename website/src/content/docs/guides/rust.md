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

Create a `Vorpal.toml` manifest in your project root:

```toml title="Vorpal.toml"
language = "rust"
name = "my-config"

[source]
includes = ["src", "Cargo.lock", "Cargo.toml"]

[source.rust]
packages = ["my-config"]
```

The `language` field tells Vorpal which SDK to use. The `name` field sets the config binary name. The `[source]` section defines which files to include, and `[source.rust]` lists the workspace packages needed to compile the config.

Then create a build configuration in `src/main.rs`:

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

Artifacts are the core building blocks in Vorpal. Each artifact defines what to build, which platforms to target, what files to include, and more.

### Define an artifact

Use the `Rust` builder to compile a Rust project into a cross-platform artifact:

:::note
`Rust` is a language-specific abstraction over the generic [Artifact](/concepts/artifacts/) type.
:::

```rust title="src/main.rs" {4,14-18}
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

The `Rust` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available during build |
| `with_bins(bins)` | Binary targets to produce |
| `with_check(bool)` | Enable `cargo check` |
| `with_environments(vars)` | Environment variables for the build |
| `with_excludes(patterns)` | Files to exclude from source |
| `with_format(bool)` | Enable `cargo fmt --check` |
| `with_includes(paths)` | Source files to include |
| `with_lint(bool)` | Enable `clippy` linting |
| `with_packages(pkgs)` | Workspace packages to build |
| `with_secrets(pairs)` | Build-time secrets |
| `with_source(source)` | Custom artifact source |
| `with_tests(bool)` | Enable `cargo test` |

See [Artifacts](/concepts/artifacts/) to learn more.

### Define artifact dependencies

Build artifacts like `protoc` and pass them as dependencies to your language artifact:

:::note
`Protoc` is a built-in artifact provided by the Vorpal SDK. See [Built-in artifacts](/concepts/artifacts/#built-in-artifacts) for the full list.
:::

```rust title="src/main.rs" {14,17}
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

    let protoc = ctx.fetch_artifact_alias("protoc:34.0").await?;

    Rust::new("my-app", systems)
        .with_artifacts(vec![protoc])
        .with_bins(vec!["my-app"])
        .with_includes(vec!["src", "Cargo.lock", "Cargo.toml"])
        .build(ctx).await?;

    ctx.run().await
}
```

The dependent artifact's output is available at `$VORPAL_ARTIFACT_<digest>` during execution. Use `get_env_key` to resolve the path.

See [Artifacts](/concepts/artifacts/) to learn more.

### Define development environments

Create a portable development shell with pinned tools, environment variables, and more:

```rust title="src/main.rs" {4,22-25}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::language::rust::{Rust, RustDevelopmentEnvironment},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    let protoc = ctx.fetch_artifact_alias("protoc:34.0").await?;

    Rust::new("my-app", systems.clone())
        .with_bins(vec!["my-app"])
        .with_includes(vec!["src", "Cargo.lock", "Cargo.toml"])
        .with_artifacts(vec![protoc.clone()])
        .build(ctx).await?;

    RustDevelopmentEnvironment::new("my-project-shell", systems)
        .with_artifacts(vec![protoc])
        .with_environments(vec!["RUST_LOG=debug".into(), "RUST_BACKTRACE=1".into()])
        .build(ctx).await?;

    ctx.run().await
}
```

Activate the environment:

```bash title="Terminal"
source $(vorpal build --path my-project-shell)/bin/activate
```

Verify that dependencies are coming from the Vorpal store:

```bash title="Terminal"
$ which protoc
/var/lib/vorpal/store/artifact/output/library/512b7dd.../bin/protoc
```

To exit, run `deactivate` or close the shell.

The development environment builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available in the shell |
| `with_environments(environments)` | Environment variables set in the shell |
| `without_protoc()` | Exclude the default Protoc artifact |
| `with_secrets(secrets)` | Secrets available in the shell |

See [Environments](/concepts/environments/) to learn more.

### Define jobs

Jobs run scripts that never cache by default — ideal for CI tasks, tests, and automation.

```rust title="src/main.rs" {4,27,29-31}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{get_env_key, language::rust::{Rust, RustDevelopmentEnvironment}, Job},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    let protoc = ctx.fetch_artifact_alias("protoc:34.0").await?;

    let my_app = Rust::new("my-app", systems.clone())
        .with_artifacts(vec![protoc.clone()])
        .with_bins(vec!["my-app"])
        .with_includes(vec!["src", "Cargo.lock", "Cargo.toml"])
        .build(ctx).await?;

    RustDevelopmentEnvironment::new("my-project-shell", systems.clone())
        .with_artifacts(vec![protoc])
        .with_environments(vec!["RUST_LOG=debug".into(), "RUST_BACKTRACE=1".into()])
        .build(ctx).await?;

    let script = format!("{}/bin/my-app --version", get_env_key(&my_app));

    Job::new("my-job", script, systems)
        .with_artifacts(vec![my_app])
        .build(ctx).await?;

    ctx.run().await
}
```

The `Job` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_secrets(secrets)` | Secrets available during execution |

See [Jobs](/concepts/jobs/) to learn more.

### Define processes

Processes wrap long-running binaries with start, stop, and logs lifecycle scripts.

```rust title="src/main.rs" {4,33-40}
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    artifact::{get_env_key, language::rust::{Rust, RustDevelopmentEnvironment}, Job, Process},
    context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;

    let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

    let protoc = ctx.fetch_artifact_alias("protoc:34.0").await?;

    let my_app = Rust::new("my-app", systems.clone())
        .with_artifacts(vec![protoc.clone()])
        .with_bins(vec!["my-app"])
        .with_includes(vec!["src", "Cargo.lock", "Cargo.toml"])
        .build(ctx).await?;

    RustDevelopmentEnvironment::new("my-project-shell", systems.clone())
        .with_artifacts(vec![protoc])
        .with_environments(vec!["RUST_LOG=debug".into(), "RUST_BACKTRACE=1".into()])
        .build(ctx).await?;

    let script = format!("{}/bin/my-app --version", get_env_key(&my_app));

    Job::new("my-job", script, systems.clone())
        .with_artifacts(vec![my_app.clone()])
        .build(ctx).await?;

    Process::new(
        "my-server",
        &format!("{}/bin/my-server", get_env_key(&my_app)),
        systems,
    )
    .with_arguments(vec!["--port", "8080"])
    .with_artifacts(vec![my_app])
    .build(ctx).await?;

    ctx.run().await
}
```

The `Process` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_arguments(arguments)` | Command-line arguments for the process |
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_secrets(secrets)` | Secrets available during execution |

See [Processes](/concepts/processes/) to learn more.

### Define user environments

Install tools into your user-wide environment with symlinks:

```rust title="src/main.rs" {4,14-16}
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

The `UserEnvironment` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available in the environment |
| `with_environments(environments)` | Environment variables set in the environment |
| `with_symlinks(symlinks)` | Symlinks to create from artifact outputs to local paths |

See [Environments](/concepts/environments/) to learn more.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```rust title="src/main.rs" {3-4,14-19,21-22}
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

The `ArtifactStep` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_arguments(arguments)` | Arguments passed to the entrypoint |
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_environments(environments)` | Environment variables for the step |
| `with_script(script)` | Script to execute in the step |
| `with_secrets(secrets)` | Secrets available during execution |

The `Artifact` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_aliases(aliases)` | Alternative names for the artifact |
| `with_sources(sources)` | Source files to include in the artifact |

See [Artifacts](/concepts/artifacts/) to learn more.

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.
