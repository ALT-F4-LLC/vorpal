---
title: TypeScript SDK
description: Build artifacts and environments with the Vorpal TypeScript SDK.
---

The TypeScript SDK lets you define Vorpal build configurations as TypeScript programs. Your config runs with Bun and communicates with the Vorpal daemon over gRPC.

## Installation

Install the SDK from npm:

```bash
npm install @altf4llc/vorpal-sdk
```

Or with Bun:

```bash
bun add @altf4llc/vorpal-sdk
```

## Project setup

Create a build configuration in `vorpal.ts`:

```typescript title="vorpal.ts"
import { ArtifactSystem, ConfigContext } from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

// Define your artifacts here

await context.run();
```

Every Vorpal config starts by creating a context and defining target systems. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

### Build a TypeScript project

Use the `TypeScript` builder to compile a TypeScript project into a cross-platform artifact:

```typescript title="vorpal.ts" {1,13-17}
import { ArtifactSystem, ConfigContext, TypeScript } from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new TypeScript("my-app", SYSTEMS)
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await context.run();
```

The `TypeScript` builder:
- **`withEntrypoint`** — Sets the entry file. When set, produces a compiled binary. When omitted, produces a library package.
- **`withIncludes`** — Lists files and directories to include in the build source

### Development environments

Create a portable development shell with pinned tools and environment variables:

```typescript title="vorpal.ts" {5,13-15}
import {
  ArtifactSystem,
  ConfigContext,
  DevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new DevelopmentEnvironment("my-project", SYSTEMS)
  .withEnvironments(["FOO=bar"])
  .build(context);

await context.run();
```

Activate the environment by sourcing the generated `bin/activate` script inside the artifact output.

### User environments

Install tools into your user-wide environment with symlinks:

```typescript title="vorpal.ts" {5,13-15}
import {
  ArtifactSystem,
  ConfigContext,
  UserEnvironment,
} from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new UserEnvironment("my-home", SYSTEMS)
  .withSymlinks([["/path/to/local/bin/app", "$HOME/.vorpal/bin/app"]])
  .build(context);

await context.run();
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```typescript title="vorpal.ts" {4-5,13-21}
import {
  ArtifactSystem,
  ConfigContext,
  Artifact,
  ArtifactStep,
} from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

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
```

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.

## Common patterns

### Builder options

The `TypeScript` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withEntrypoint(file)` | Entry file — binary mode when set, library mode when omitted |
| `withIncludes(paths)` | Source files to include |
| `withArtifacts(names)` | Artifact dependencies available during build |
| `withEnvironments(vars)` | Environment variables for the build |
| `withSecrets(map)` | Build-time secrets |
| `withAliases(aliases)` | Alternative names for the artifact |
| `withSourceScripts(scripts)` | Scripts to run before build |
| `withWorkingDir(dir)` | Custom working directory |

### Multiple artifacts

Chain multiple artifacts in a single config — they share the same context and build graph:

```typescript
await new TypeScript("lib-core", SYSTEMS)
  .withIncludes(["packages/core", "bun.lock", "package.json"])
  .build(context);

await new TypeScript("bin-server", SYSTEMS)
  .withEntrypoint("packages/server/src/main.ts")
  .withIncludes(["packages/server", "bun.lock", "package.json"])
  .build(context);

await new TypeScriptDevelopmentEnvironment("dev-shell", SYSTEMS)
  .build(context);
```
