---
title: Jobs
description: Script-based build units for running one-off tasks in Vorpal.
---

A **Job** is a script-based build unit that wraps a shell command into a content-addressed artifact. Jobs are the simplest way to run a one-off task -- a version check, a test suite, a data migration, or any command that runs to completion and exits.

## How jobs work

A Job takes a name, a shell script, and a list of target systems. Under the hood, it creates an [artifact](/concepts/artifacts/) with a single shell step that executes your script. The output is content-addressed and cached like any other Vorpal artifact.

```mermaid
flowchart TD
    A["Job(name, script, systems)"] --> B["Artifact with single shell step"]
    B --> C["Content-addressed, cached output"]
```

## When to use a Job

Use a Job when you need to:

- Run a command that produces output and exits (not a long-running service)
- Wrap existing CLI tools into reproducible, cached build steps
- Compose multiple artifacts together into a single execution

If your task needs to run continuously (a server, a watcher, a daemon), use a [Process](/concepts/processes/) instead.

## Defining a Job

### Rust

```rust
use vorpal_sdk::{
    artifact::{get_env_key, system, Job},
    context::get_context,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut ctx = get_context().await?;

    let tool = build_my_tool(&mut ctx).await?;
    let script = format!("{}/bin/my-tool --version", get_env_key(&tool));

    Job::new("my-job", script, system::SYSTEMS)
        .with_artifacts(vec![tool])
        .build(&mut ctx)
        .await?;

    ctx.run().await
}
```

### Go

```go
script := fmt.Sprintf("%s/bin/my-tool --version", artifact.GetEnvKey(*tool))

artifact.NewJob("my-job", script, config.SYSTEMS).
    WithArtifacts([]*string{tool}).
    Build(ctx)
```

### TypeScript

```typescript
import {
  ConfigContext,
  Job,
  SYSTEMS,
  getEnvKey,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const tool = await buildMyTool(context);
const script = `${getEnvKey(tool)}/bin/my-tool --version`;

await new Job("my-job", script, SYSTEMS)
  .withArtifacts([tool])
  .build(context);

await context.run();
```

## Builder options

| Method | Description |
|--------|-------------|
| `new(name, script, systems)` | Create a Job with a name, shell script, and target platforms |
| `with_artifacts(artifacts)` | Add dependency artifacts whose outputs are available during execution |
| `with_secrets(secrets)` | Add encrypted secrets, each read at runtime from the file `$VORPAL_SECRET_DIR/<NAME>` and not from an environment variable |

### Secrets

Each secret is delivered to the job's step as a file at `$VORPAL_SECRET_DIR/<NAME>`. Read the value from that file in the script. A step whose arguments or environment reference a secret (`$NAME` or `${NAME}`) fails. Secrets follow these rules:

- A value is at most 64 KiB (65536 bytes) per secret and must be valid UTF-8.
- A name matches `[A-Za-z_][A-Za-z0-9_]*`, does not start with `VORPAL_`, and is not a duplicate of another secret name in the same step.
- Do not copy a secret file into `$VORPAL_OUTPUT`, because outputs are pushed.

## How it differs from an Artifact

A Job is a thin convenience wrapper around an [Artifact](/concepts/artifacts/). Under the hood, `Job.build()` creates an Artifact with a single shell step and calls `Artifact.build()`, so Jobs go through the same content-addressed pipeline -- identical inputs produce a cache hit and the build is skipped.

The difference is ergonomic, not behavioral:

- An **Artifact** gives you full control over multiple steps, sources, and build logic.
- A **Job** provides a streamlined API for the common case of running a single shell script with optional artifact dependencies and secrets.

### CI and automation as code

The bigger picture behind Jobs is that they let you define your entire CI and automation pipeline as code -- in the same language as your build config -- and run it anywhere. Locally, in GitHub Actions, on a remote server, or on any machine with Vorpal installed.

Instead of maintaining platform-specific YAML for each CI provider, you write your pipeline once as Jobs and execute them everywhere. This is similar in spirit to projects like [Dagger](https://dagger.io), where your pipeline is a real program, not a config file.
