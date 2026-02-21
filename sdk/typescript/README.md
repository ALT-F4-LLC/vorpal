# @vorpal/sdk

TypeScript SDK for [Vorpal](https://github.com/ALT-F4-LLC/vorpal) -- build and ship software with one language-agnostic workflow.

Define Vorpal artifact configurations in TypeScript with full type safety, editor autocomplete, and the same builder pattern used by the Rust and Go SDKs.

## Installation

```bash
# Using bun (recommended -- Vorpal uses Bun as the TypeScript runtime)
bun add @vorpal/sdk

# Using npm
npm install @vorpal/sdk
```

## Quick Start

### 1. Create a Vorpal project

```bash
mkdir my-project && cd my-project
vorpal artifact init   # select TypeScript when prompted
```

Or manually create the following files:

**Vorpal.toml**
```toml
language = "typescript"
name = "my-config"

[source]
includes = ["src", "package.json", "tsconfig.json", "bun.lockb"]
```

**package.json**
```json
{
  "name": "my-vorpal-config",
  "private": true,
  "type": "module",
  "dependencies": {
    "@vorpal/sdk": "0.1.0"
  }
}
```

**tsconfig.json**
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ES2022",
    "moduleResolution": "bundler",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "outDir": "dist",
    "declaration": true,
    "types": ["bun-types"]
  },
  "include": ["src"]
}
```

### 2. Write your config

**src/vorpal.ts**
```typescript
import {
  ConfigContext,
  ArtifactSystem,
  JobBuilder,
  ProjectEnvironmentBuilder,
} from "@vorpal/sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.AARCH64_LINUX,
  ArtifactSystem.X8664_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

async function main() {
  const context = ConfigContext.create();

  // Define a build job
  const buildDigest = await new JobBuilder(
    "my-build",
    `
      echo "Building project..."
      cp -r source/* $VORPAL_OUTPUT/
    `,
    SYSTEMS,
  ).build(context);

  // Define a development shell
  await new ProjectEnvironmentBuilder("my-shell", SYSTEMS)
    .withArtifacts([buildDigest])
    .withEnvironments(["NODE_ENV=development"])
    .build(context);

  // Start the context service (required -- the CLI connects to this)
  await context.run();
}

main().catch((e) => { console.error(e); process.exit(1); });
```

### 3. Build

```bash
vorpal build "my-config"
```

## API Reference

All public exports are available from `@vorpal/sdk`:

```typescript
import {
  // Context
  ConfigContext,
  parseArtifactAlias,
  formatArtifactAlias,

  // Builders
  ArtifactBuilder,
  ArtifactSourceBuilder,
  ArtifactStepBuilder,
  Argument,
  JobBuilder,
  ProcessBuilder,
  ProjectEnvironmentBuilder,
  TypeScriptBuilder,
  UserEnvironmentBuilder,
  getEnvKey,

  // Step functions
  bash,
  bwrap,
  shell,
  docker,

  // System utilities
  getSystem,
  getSystemDefault,
  getSystemDefaultStr,
  getSystemStr,

  // CLI
  parseCliArgs,

  // Generated protobuf types
  ArtifactSystem,
} from "@vorpal/sdk";
```

### ConfigContext

The central coordinator that manages gRPC connections to Vorpal services and stores artifacts.

```typescript
// Create a context (parses CLI args, connects to agent and registry)
const context = ConfigContext.create();

// Add an artifact (serializes, computes SHA-256 digest, sends to agent)
const digest: string = await context.addArtifact(artifact);

// Fetch an artifact by digest from the registry
await context.fetchArtifact(digest);

// Fetch an artifact by alias (e.g., "library/linux-vorpal:latest")
const aliasDigest: string = await context.fetchArtifactAlias("my-tool:v1.0");

// Read context state
context.getSystem();            // ArtifactSystem enum value
context.getArtifactName();      // artifact name from CLI args
context.getArtifactNamespace(); // artifact namespace from CLI args
context.getVariable("key");     // variable value or undefined

// Start the ContextService gRPC server (must be called last)
await context.run();
```

### Builders

All builders follow the same pattern: construct with required fields, chain `with*` methods for optional fields, and call `.build(context)` to register the artifact. Each `.build()` call returns a `Promise<string>` containing the artifact's SHA-256 digest.

#### JobBuilder

Runs a shell script as a build step.

```typescript
const digest = await new JobBuilder("my-job", "echo hello", SYSTEMS)
  .withArtifacts([depDigest1, depDigest2])       // artifact dependencies
  .withSecrets([["API_KEY", process.env.API_KEY!]]) // secret name-value pairs
  .build(context);
```

#### ProcessBuilder

Creates a managed background process with start/stop/logs helper scripts.

```typescript
const digest = await new ProcessBuilder("my-server", "node", SYSTEMS)
  .withArguments(["server.js", "--port", "3000"])
  .withArtifacts([nodeDigest])
  .withSecrets([["DB_URL", process.env.DB_URL!]])
  .build(context);
```

#### ProjectEnvironmentBuilder

Creates a development environment with a `bin/activate` script that configures PATH and environment variables.

```typescript
const digest = await new ProjectEnvironmentBuilder("my-devenv", SYSTEMS)
  .withArtifacts([toolDigest1, toolDigest2])
  .withEnvironments(["NODE_ENV=development", "FOO=bar"])
  .withSecrets([["TOKEN", process.env.TOKEN!]])
  .build(context);
```

#### UserEnvironmentBuilder

Creates a user-wide environment with symlink management under `$HOME/.vorpal/bin`.

```typescript
const digest = await new UserEnvironmentBuilder("my-userenv", SYSTEMS)
  .withArtifacts([toolDigest])
  .withEnvironments(["EDITOR=nvim"])
  .withSymlinks([["/path/to/source/bin", "$HOME/.vorpal/bin/my-tool"]])
  .build(context);
```

#### TypeScriptBuilder

Compiles a TypeScript/Node.js project into a standalone binary using [Bun](https://bun.sh/). By default, the Bun toolchain is fetched from the registry automatically (`bun:1.2.0`). Use `.withBun(digest)` to override with a custom Bun artifact.

```typescript
// Minimal -- Bun is fetched from the registry automatically
const digest = await new TypeScriptBuilder("my-app", SYSTEMS)
  .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
  .build(context);

// With options
const digest = await new TypeScriptBuilder("my-app", SYSTEMS)
  .withBun(customBunDigest)                          // override Bun artifact
  .withEntrypoint("src/main.ts")                     // default: src/{name}.ts
  .withIncludes(["src", "package.json", "bun.lockb"])
  .withArtifacts([depDigest])                        // additional dependencies
  .withEnvironments(["NODE_ENV=production"])
  .withSecrets([["API_KEY", process.env.API_KEY!]])
  .build(context);
```

#### ArtifactBuilder

Low-level builder for custom artifacts with explicit steps and sources.

```typescript
import { ArtifactBuilder, ArtifactSourceBuilder, shell } from "@vorpal/sdk";

const source = new ArtifactSourceBuilder("my-source", ".")
  .withIncludes(["src/**/*.ts"])
  .withExcludes(["node_modules"])
  .build();

const step = await shell(context, [depDigest], [], "npm run build", []);

const digest = await new ArtifactBuilder("my-artifact", [step], SYSTEMS)
  .withSources([source])
  .withAliases(["my-org/my-artifact:v1.0"])
  .build(context);
```

#### ArtifactStepBuilder

Low-level builder for custom step definitions with arbitrary entrypoints.

```typescript
import { ArtifactStepBuilder } from "@vorpal/sdk";

const step = new ArtifactStepBuilder("docker")
  .withArguments(["run", "--rm", "-v", "$VORPAL_OUTPUT:/out", "alpine", "sh", "-lc", "echo hi > /out/hi.txt"])
  .withArtifacts([depDigest])
  .withEnvironments(["FOO=bar"])
  .build();
```

#### Argument

Reads build-time variables passed via `--artifact-variable KEY=VALUE`.

```typescript
import { Argument } from "@vorpal/sdk";

const version = new Argument("VERSION").withRequire().build(context);
// Throws if --artifact-variable VERSION=... was not provided
```

### Step Functions

Convenience functions for creating artifact steps.

| Function | Description |
|----------|-------------|
| `bash(artifacts, environments, secrets, script)` | Bash step with PATH from artifact bins |
| `shell(context, artifacts, environments, script, secrets)` | Bash on macOS, Bubblewrap sandbox on Linux |
| `bwrap(args, artifacts, environments, rootfs, secrets, script)` | Bubblewrap sandbox step (Linux) |
| `docker(args, artifacts)` | Docker container step |

`shell()` is the recommended default -- it automatically sandboxes on Linux using Bubblewrap while falling back to plain Bash on macOS.

### Utility Functions

```typescript
// Get the environment variable reference for an artifact digest
getEnvKey("abc123");  // "$VORPAL_ARTIFACT_abc123"

// Parse/format artifact aliases
parseArtifactAlias("my-org/tool:v2.0");
// { name: "tool", namespace: "my-org", tag: "v2.0" }

formatArtifactAlias({ name: "tool", namespace: "library", tag: "latest" });
// "tool"

// System detection
getSystemDefault();     // ArtifactSystem enum for current platform
getSystemDefaultStr();  // e.g., "aarch64-darwin"
getSystem("x86_64-linux"); // ArtifactSystem.X8664_LINUX
getSystemStr(ArtifactSystem.AARCH64_DARWIN); // "aarch64-darwin"
```

### ArtifactSystem Enum

```typescript
enum ArtifactSystem {
  UNKNOWN_SYSTEM = 0,
  AARCH64_DARWIN = 1,
  AARCH64_LINUX = 2,
  X8664_DARWIN = 3,
  X8664_LINUX = 4,
}
```

## How It Works

The TypeScript SDK follows the same config-as-code pattern as the Rust and Go SDKs:

1. Vorpal compiles your TypeScript config to a standalone binary using [Bun](https://bun.sh/).
2. The binary parses CLI arguments and connects to the Vorpal agent and registry via gRPC.
3. Each `.build(context)` call serializes the artifact to JSON, computes a SHA-256 digest, and sends it to the agent for preparation.
4. After all artifacts are defined, `context.run()` starts a gRPC server that the CLI queries to retrieve the artifact graph.
5. The CLI topologically sorts the graph and builds each artifact through the worker service.

Artifact digests are computed identically across all three SDKs (Rust, Go, TypeScript), so artifacts are fully interoperable and cacheable.

## Links

- [Vorpal repository](https://github.com/ALT-F4-LLC/vorpal)
- [Main documentation](https://github.com/ALT-F4-LLC/vorpal#readme)
- [Design specification](https://github.com/ALT-F4-LLC/vorpal/blob/main/docs/design/typescript-sdk.md)

## License

Apache-2.0
