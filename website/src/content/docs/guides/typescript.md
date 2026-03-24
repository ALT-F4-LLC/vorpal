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

Create a `Vorpal.toml` manifest in your project root:

```toml title="Vorpal.toml"
language = "typescript"

[source]
includes = ["src", "bun.lock", "package.json", "tsconfig.json"]

[source.typescript]
directory = "."
entrypoint = "src/vorpal.ts"
```

The `language` field tells Vorpal which SDK to use. The `[source]` section defines which files to include, and `[source.typescript]` sets the directory and entrypoint for your build config.

Then create a build configuration in `vorpal.ts`:

```typescript title="vorpal.ts"
import { ArtifactSystem, ConfigContext } from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

// Define your artifacts here

await context.run();
```

Every Vorpal config starts by creating a context and defining target systems. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

Artifacts are the core building blocks in Vorpal. Each artifact defines what to build, which platforms to target, what files to include, and more.

### Define an artifact

Use the `TypeScript` builder to compile a TypeScript project into a cross-platform artifact:

:::note
`TypeScript` is a language-specific abstraction over the generic [Artifact](/concepts/artifacts/) type.
:::

```typescript title="vorpal.ts" {1,12-15}
import { ArtifactSystem, ConfigContext, TypeScript } from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

await new TypeScript("my-app", SYSTEMS)
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await context.run();
```

The `TypeScript` builder:
- **`withEntrypoint`** — Sets the entry file. When set, produces a compiled binary. When omitted, produces a library package.
- **`withIncludes`** — Lists files and directories to include in the build source

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

See [Artifacts](/concepts/artifacts/) to learn more.

### Define artifact dependencies

Build artifacts like `Protoc` and pass them as dependencies to your language artifact:

:::note
`Protoc` is a built-in artifact provided by the Vorpal SDK. See [Built-in artifacts](/concepts/artifacts/#built-in-artifacts) for the full list.
:::

```typescript title="vorpal.ts" {14,17}
import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const protoc = await context.fetchArtifactAlias("protoc:34.0");

await new TypeScript("my-app", SYSTEMS)
  .withArtifacts([protoc])
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await context.run();
```

The dependent artifact's output is available at `$VORPAL_ARTIFACT_<digest>` during execution. Use `getEnvKey` to resolve the path.

See [Artifacts](/concepts/artifacts/) to learn more.

### Define development environments

Create a portable development shell with pinned tools, environment variables, and more:

```typescript title="vorpal.ts" {5,20-23}
import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
  TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const protoc = await context.fetchArtifactAlias("protoc:34.0");

await new TypeScript("my-app", SYSTEMS)
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .withArtifacts([protoc])
  .build(context);

await new TypeScriptDevelopmentEnvironment("my-project-shell", SYSTEMS)
  .withArtifacts([protoc])
  .withEnvironments(["NODE_ENV=development", "LOG_LEVEL=debug"])
  .build(context);

await context.run();
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
| `withArtifacts(artifacts)` | Artifact dependencies available in the shell |
| `withEnvironments(environments)` | Environment variables set in the shell |
| `withSecrets(secrets)` | Secrets available in the shell |

See [Environments](/concepts/environments/) to learn more.

### Define jobs

Jobs run scripts that never cache by default — ideal for CI tasks, tests, and automation.

```typescript title="vorpal.ts" {6-7,30,32-34}
import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
  TypeScriptDevelopmentEnvironment,
  getEnvKey,
  Job,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const protoc = await context.fetchArtifactAlias("protoc:34.0");

const myApp = await new TypeScript("my-app", SYSTEMS)
  .withArtifacts([protoc])
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await new TypeScriptDevelopmentEnvironment("my-project-shell", SYSTEMS)
  .withArtifacts([protoc])
  .withEnvironments(["NODE_ENV=development", "LOG_LEVEL=debug"])
  .build(context);

const script = `${getEnvKey(myApp)}/bin/my-app --version`;

await new Job("my-job", script, SYSTEMS)
  .withArtifacts([myApp])
  .build(context);

await context.run();
```

The `Job` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withArtifacts(artifacts)` | Artifact dependencies available during execution |
| `withSecrets(secrets)` | Secrets available during execution |

See [Jobs](/concepts/jobs/) to learn more.

### Define processes

Processes wrap long-running binaries with start, stop, and logs lifecycle scripts.

```typescript title="vorpal.ts" {8,36-43}
import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
  TypeScriptDevelopmentEnvironment,
  getEnvKey,
  Job,
  Process,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const protoc = await context.fetchArtifactAlias("protoc:34.0");

const myApp = await new TypeScript("my-app", SYSTEMS)
  .withArtifacts([protoc])
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await new TypeScriptDevelopmentEnvironment("my-project-shell", SYSTEMS)
  .withArtifacts([protoc])
  .withEnvironments(["NODE_ENV=development", "LOG_LEVEL=debug"])
  .build(context);

const script = `${getEnvKey(myApp)}/bin/my-app --version`;

await new Job("my-job", script, SYSTEMS)
  .withArtifacts([myApp])
  .build(context);

await new Process(
  "my-server",
  `${getEnvKey(myApp)}/bin/my-server`,
  SYSTEMS,
)
  .withArguments(["--port", "8080"])
  .withArtifacts([myApp])
  .build(context);

await context.run();
```

The `Process` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withArguments(args)` | Command-line arguments for the process |
| `withArtifacts(artifacts)` | Artifact dependencies available during execution |
| `withSecrets(secrets)` | Secrets available during execution |

See [Processes](/concepts/processes/) to learn more.

### Define user environments

Install tools into your user-wide environment with symlinks:

```typescript title="vorpal.ts" {4-5,17-21,23-25}
import {
  ArtifactSystem,
  ConfigContext,
  TypeScript,
  getEnvKey,
  UserEnvironment,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const myApp = await new TypeScript("my-app", SYSTEMS)
  .withEntrypoint("src/main.ts")
  .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
  .build(context);

await new UserEnvironment("my-home", SYSTEMS)
  .withArtifacts([myApp])
  .withSymlinks([[`${getEnvKey(myApp)}/bin/my-app`, "$HOME/.vorpal/bin/my-app"]])
  .build(context);

await context.run();
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

The `UserEnvironment` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withArtifacts(artifacts)` | Artifact dependencies available in the environment |
| `withEnvironments(environments)` | Environment variables set in the environment |
| `withSymlinks(symlinks)` | Symlinks to create from artifact outputs to local paths |

See [Environments](/concepts/environments/) to learn more.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```typescript title="vorpal.ts" {4-5,18-26}
import {
  ArtifactSystem,
  ConfigContext,
  Artifact,
  ArtifactStep,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

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

The `ArtifactStep` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withArguments(args)` | Arguments passed to the entrypoint |
| `withArtifacts(artifacts)` | Artifact dependencies available during execution |
| `withEnvironments(environments)` | Environment variables for the step |
| `withScript(script)` | Script to execute in the step |
| `withSecrets(secrets)` | Secrets available during execution |

The `Artifact` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `withAliases(aliases)` | Alternative names for the artifact |
| `withSources(sources)` | Source files to include in the artifact |

See [Artifacts](/concepts/artifacts/) to learn more.

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.
