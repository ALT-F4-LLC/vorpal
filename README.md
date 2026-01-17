# Vorpal

Build and ship software with one language-agnostic workflow.

## Why?
- Declarative: describe steps once, use them anywhere.
- Cross-language: Rust and Go SDKs today; more to come.
- Reproducible: hermetic steps and pinned toolchains.
- Scalable: the same artifacts power your end-to-end flow.

## Architecture
Vorpal is distributed and composed of horizontally scalable components:

- CLI (orchestrator): runs builds and talks to services over gRPC.
- Agent service (localhost): performs filesystem/sandbox tasks close to the workload.
- Registry service (storage): persists artifacts and metadata (e.g., S3-backed in CI).
- Worker service (executor): executes steps in isolated environments; scale by adding workers.

Run services locally during development with `make vorpal-start` (or `cargo run --bin vorpal -- services start`).

```mermaid
flowchart LR
  Agent -- "Read & Write" --> Sandbox
  Agent -- "Pull & Push" --> Registry
  Registry -- "Read & Write" --> Store
  Worker -- "Read & Write" --> Sandbox
  Worker -- "Pull & Push" --> Registry

  CLI -- "GetArtifacts" --> SDK
  SDK -- "FetchArtifact" --> Registry
  CLI -- "PrepareArtifact" --> Agent
  CLI -- "BuildArtifact" --> Worker
  Store --> ObjectStorage(Object Storage)
```

## Setup
### Install (prebuilt binaries):
  - `curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/refs/heads/main/script/install.sh -o install.sh && sh install.sh`

### Build from source (macOS & Linux):
  - macOS only (once): `xcode-select --install`
  - All platforms: `./script/dev.sh make build` (preferred; installs and uses a consistent toolchain)
  - Common tasks: `make check`, `make test`, `make format`, `make lint`, `make dist`

## Using the SDK
The examples below build a simple Rust binary artifact for multiple systems and run the context.

### Rust
```rust
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

    Rust::new("example", systems).build(ctx).await?;

    ctx.run().await
}
```

### Go
```go
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    language.NewGo("example", systems).Build(ctx)

    ctx.Run()
}
```

## Quickstart
These steps assume you installed Vorpal via the installer and have `vorpal` on your PATH.

1) One-time keys

- `vorpal system keys generate`  # installer runs this; safe to re-run

2) Start services (agent, registry, worker)

- If you used the installer, services are already running.
- Otherwise: `vorpal services start`  # defaults to https://localhost:23151

3) Create a new project (pick Go or Rust)

- `mkdir hello-vorpal && cd hello-vorpal`
- `vorpal artifact init`  # scaffolds Vorpal.toml and a sample

4) Build your artifact

- `vorpal build "vorpal"`  # builds using the local services
- To get the output path: `vorpal build --path "vorpal"`

5) Run the sample

- `ARTIFACT_PATH=$(vorpal build --path "vorpal")`
- `$ARTIFACT_PATH/bin/example`  # runs the generated example binary

Build this repository

- From the repo root: `vorpal build "vorpal"`
- Optional Go parity (if present): `vorpal build --config "Vorpal.go.toml" "vorpal"`

## Dev & User Environments
Manage development and user-wide environments using the builders:

- Development environment (devenv): creates a portable shell activation (`bin/activate`) that prepends tool artifacts to PATH and sets env vars.
- User environment (userenv): installs activation helpers and safe symlinking under `$HOME/.vorpal/bin`.

**Rust**
```rust
use anyhow::Result;
use vorpal_sdk::{
  api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
  artifact::{ProjectEnvironment, UserEnvironment},
  context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
  let ctx = &mut get_context().await?;
  let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

  ProjectEnvironment::new("my-project", systems.clone())
    .with_environments(vec!["FOO=bar".into()])
    .build(ctx).await?;

  UserEnvironment::new("my-home", systems)
    .with_symlinks(vec![("/path/to/local/bin/app", "$HOME/.vorpal/bin/app")])
    .build(ctx).await?;

  ctx.run().await
}
```

**Go**
```go
package main

import (
  api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
  "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
  "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
  api.ArtifactSystem_AARCH64_DARWIN,
  api.ArtifactSystem_AARCH64_LINUX,
  api.ArtifactSystem_X8664_DARWIN,
  api.ArtifactSystem_X8664_LINUX,
}

func main() {
  ctx := config.GetContext()

  artifact.NewProjectEnvironment("my-project", systems).
    WithEnvironments([]string{"FOO=bar"}).
    Build(ctx)

  artifact.NewUserEnvironment("my-home", systems).
    WithSymlinks(map[string]string{"/path/to/local/bin/app": "$HOME/.vorpal/bin/app"}).
    Build(ctx)

  ctx.Run()
}
```

### Activate
- Development environments: source generated `bin/activate` inside the artifact output when used within a step or your own wrapper script.
- User environments: run `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

## Executors
Vorpal does not lock you to a single executor. Each step sets its executor via `artifact.step[].entrypoint` and `artifact.step[].arguments`.

- Default: Bash. SDK “shell” helpers run in Bash (on Linux these run inside Bubblewrap).
- Custom: Point `entrypoint` to any binary (e.g., `bwrap`, `docker`, `podman`) and pass flags via `arguments`.

**Rust (custom entrypoint/arguments)**
```rust
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

    Artifact::new("example-docker", vec![step], systems).build(ctx).await?;

    ctx.run().await
}
```

**Go (custom entrypoint/arguments)**
```go
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    step, _ := artifact.NewArtifactStep().
        WithEntrypoint("docker", systems).
        WithArguments([]string{"run", "--rm", "-v", "$VORPAL_OUTPUT:/out", "alpine", "sh", "-lc", "echo hi > /out/hi.txt"}, systems).
        Build(ctx)

    artifact.NewArtifact("example-docker", []*api.ArtifactStep{step}, systems).Build(ctx)

    ctx.Run()
}
```

## Templates (via `vorpal artifact init`)
These are excerpts from the generated templates so you can map scaffolding to SDK usage.

**Rust template** (`cli/src/command/template/rust/src/vorpal.rs`)
```rust
Rust::new("example", SYSTEMS.to_vec())
    .with_bins(vec!["example"])
    .with_includes(vec!["src/main.rs", "Cargo.lock", "Cargo.toml"])
    .build(context)
    .await?;
```

**Go template** (`cli/src/command/template/go/cmd/vorpal/main.go`)
```go
artifact.NewProjectEnvironment("example-dev", Systems).
    WithArtifacts([]*string{gobin, goimports, gopls, protoc, protocGenGo, protocGenGoGRPC, staticcheck}).
    WithEnvironments([]string{fmt.Sprintf("GOARCH=%s", *goarch), fmt.Sprintf("GOOS=%s", *goos)}).
    Build(context)

language.NewGo("example", Systems).
    WithBuildDirectory("cmd/example").
    WithIncludes([]string{"cmd/example", "go.mod", "go.sum"}).
    Build(context)
```

## Remix an existing artifact
This mirrors the “fetch, tweak, and rebuild” flow. If you want to modify an existing artifact, you can do so by fetching it, tweaking it, and adding it back to the context.

**Rust**
```rust
use anyhow::Result;
use vorpal_sdk::{context::get_context, api::artifact::Artifact};

#[tokio::main]
async fn main() -> Result<()> {
    let ctx = &mut get_context().await?;
    let git_digest = ctx.fetch_artifact("<digest>").await?;
    let mut git: Artifact = ctx.get_artifact(&git_digest).expect("git artifact");

    git.name = "my-git".to_string();
    // tweak git.steps / git.systems / git.sources as needed

    ctx.add_artifact(&git).await?;
    ctx.run().await
}
```

**Go**
```go
ctx := config.GetContext()
digest, _ := ctx.FetchArtifactAlias("library/git:latest")
git := ctx.GetArtifact(*digest)

git.Name = "my-git"
// tweak git.Steps / git.Systems / git.Sources as needed

ctx.AddArtifact(git)
ctx.Run()
```

## Artifact functions
Use artifact functions as a starting point, then customize.

**Rust**
```rust
use std::collections::HashMap;
let mut git = ctx
    .get_artifact_function("git", "library", "latest", HashMap::new())
    .await?;
git.name = "my-git".to_string();
ctx.add_artifact(&git).await?;
```

**Go**
```go
git, _ := ctx.GetArtifactFunction("git", "library", "latest", map[string]string{})
git.Name = "my-git"
ctx.AddArtifact(git)
```


## Contribute
- Read the contributor guide: `AGENTS.md` (structure, commands, style, and PR workflow).
- Before opening a PR: `make format && make lint && make test`.
- Prefer small, focused changes with clear descriptions and linked issues.
- For local development, use `./script/dev.sh` or `direnv allow` to get a consistent environment.
