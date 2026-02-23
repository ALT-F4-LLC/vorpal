# Vorpal

**Build software in Rust, Go, or TypeScript. Get reproducible artifacts on every platform.**

[![CI](https://github.com/ALT-F4-LLC/vorpal/actions/workflows/vorpal.yaml/badge.svg)](https://github.com/ALT-F4-LLC/vorpal/actions/workflows/vorpal.yaml) [![Release](https://img.shields.io/github/v/release/ALT-F4-LLC/vorpal?include_prereleases)](https://github.com/ALT-F4-LLC/vorpal/releases) [![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE) [![npm](https://img.shields.io/npm/v/@vorpal/sdk)](https://www.npmjs.com/package/@vorpal/sdk)

Vorpal is a build system that works the way you already write code. Define your build as a program -- not YAML, not a DSL -- using real SDKs in Rust, Go, or TypeScript. Vorpal handles hermetic execution, cross-platform targeting, content-addressed caching, and artifact distribution so you can focus on what you are building.

> Think Nix-level reproducibility, without learning a new language. Think Bazel-level caching, without the configuration overhead. Think Docker builds, but actually deterministic.

## Contents

- [Install](#install)
- [Quickstart](#quickstart)
- [SDK Examples](#sdk-examples)
- [Features](#features)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | sh
```

macOS (Apple Silicon, Intel) and Linux (x86_64, ARM64). The installer downloads the latest release, generates TLS keys, and starts background services.

> Building from source? See the [Contributing](#contributing) section.

## Quickstart

Create a new project and build your first artifact.

### 1. Create a project

```bash
mkdir hello-vorpal && cd hello-vorpal
vorpal init hello-vorpal
```

Choose your language (Go, Rust, or TypeScript) when prompted. Vorpal scaffolds a working project with a `Vorpal.toml` and sample build config.

### 2. Build it

```bash
vorpal build vorpal
```

Vorpal compiles your config, resolves dependencies, and produces a content-addressed artifact. First builds download toolchains; subsequent builds are cached.

### 3. Run it

```bash
vorpal run hello-vorpal
```

That is it. Your artifact is built, cached, and runnable.

## SDK Examples

Vorpal build configs are real programs. Write them in the language your project already uses.

### Build an artifact

Define a build artifact targeting multiple platforms with a single config file.

<details open>
<summary><strong>TypeScript</strong></summary>

```typescript
import { ArtifactSystem, ConfigContext, TypeScript } from "@vorpal/sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new TypeScript("example", SYSTEMS)
  .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
  .build(context);

await context.run();
```

</details>

<details>
<summary><strong>Rust</strong></summary>

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

</details>

<details>
<summary><strong>Go</strong></summary>

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

</details>

### Dev & user environments

Create portable development shells and user-wide tool installations with pinned dependencies.

<details open>
<summary><strong>TypeScript</strong></summary>

```typescript
import {
  ConfigContext,
  ArtifactSystem,
  DevelopmentEnvironment,
  UserEnvironment,
} from "@vorpal/sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

async function main() {
  const context = ConfigContext.create();

  await new DevelopmentEnvironment("my-project", SYSTEMS)
    .withEnvironments(["FOO=bar"])
    .build(context);

  await new UserEnvironment("my-home", SYSTEMS)
    .withSymlinks([["/path/to/local/bin/app", "$HOME/.vorpal/bin/app"]])
    .build(context);

  await context.run();
}

main().catch((e) => { console.error(e); process.exit(1); });
```

</details>

<details>
<summary><strong>Rust</strong></summary>

```rust
use anyhow::Result;
use vorpal_sdk::{
  api::artifact::ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
  artifact::{DevelopmentEnvironment, UserEnvironment},
  context::get_context,
};

#[tokio::main]
async fn main() -> Result<()> {
  let ctx = &mut get_context().await?;
  let systems = vec![Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

  DevelopmentEnvironment::new("my-project", systems.clone())
    .with_environments(vec!["FOO=bar".into()])
    .build(ctx).await?;

  UserEnvironment::new("my-home", systems)
    .with_symlinks(vec![("/path/to/local/bin/app", "$HOME/.vorpal/bin/app")])
    .build(ctx).await?;

  ctx.run().await
}
```

</details>

<details>
<summary><strong>Go</strong></summary>

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

  artifact.NewDevelopmentEnvironment("my-project", systems).
    WithEnvironments([]string{"FOO=bar"}).
    Build(ctx)

  artifact.NewUserEnvironment("my-home", systems).
    WithSymlinks(map[string]string{"/path/to/local/bin/app": "$HOME/.vorpal/bin/app"}).
    Build(ctx)

  ctx.Run()
}
```

</details>

**Activate:**
- Development environments: source generated `bin/activate` inside the artifact output.
- User environments: run `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

### Custom executors

Swap the default Bash executor for Docker, Bubblewrap, or any custom binary.

<details open>
<summary><strong>TypeScript</strong></summary>

```typescript
import {
  ConfigContext,
  ArtifactSystem,
  Artifact,
  ArtifactStep,
} from "@vorpal/sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

async function main() {
  const context = ConfigContext.create();

  const step = new ArtifactStep("docker")
    .withArguments([
      "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
      "alpine", "sh", "-lc", "echo hi > /out/hi.txt",
    ])
    .build();

  await new Artifact("example-docker", [step], SYSTEMS)
    .build(context);

  await context.run();
}

main().catch((e) => { console.error(e); process.exit(1); });
```

</details>

<details>
<summary><strong>Rust</strong></summary>

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

</details>

<details>
<summary><strong>Go</strong></summary>

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
        WithArguments([]string{
            "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
            "alpine", "sh", "-lc",
            "echo hi > /out/hi.txt",
        }, systems).
        Build(ctx)

    artifact.NewArtifact("example-docker",
        []*api.ArtifactStep{step}, systems).Build(ctx)

    ctx.Run()
}
```

</details>

## Features

- **Config as code** -- Your build config is a real program, not YAML. Write it in Rust, Go, or TypeScript with full IDE support.
- **Reproducible by default** -- Content-addressed artifacts with hermetic build steps. Same inputs always produce the same output.
- **Cross-platform** -- Target macOS (Apple Silicon + Intel) and Linux (x86_64 + ARM64) from a single config.
- **Built-in caching** -- Artifacts are cached by content hash. Unchanged builds resolve instantly.
- **Dev environments** -- Define project shells with pinned tools and env vars. Like direnv, but versioned and shareable.
- **Artifact registry** -- Push, pull, and share artifacts with built-in registry support. Run artifacts directly with `vorpal run`.
- **Pluggable executors** -- Build steps run in Bash by default. Swap in Docker, Bubblewrap, or any executor.

## Documentation

| Resource | Link |
|----------|------|
| Architecture overview | [`docs/spec/architecture.md`](docs/spec/architecture.md) |
| CLI reference | `vorpal --help` |
| TypeScript SDK (npm) | [`@vorpal/sdk`](https://www.npmjs.com/package/@vorpal/sdk) |
| Go SDK | [`sdk/go/`](sdk/go/) |
| Rust SDK | [`sdk/rust/`](sdk/rust/) |

## Contributing

Contributions are welcome. See [`docs/spec/`](docs/spec/) for project structure, coding standards, and review workflow.

```bash
# Build from source
./script/dev.sh make build

# Before submitting a PR
make format && make lint && make test
```

## License

[Apache 2.0](LICENSE)
