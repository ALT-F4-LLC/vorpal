---
title: CLI Reference
description: Complete reference for all vorpal CLI commands, flags, and options.
---

The `vorpal` CLI is the single entry point for all user interactions with the Vorpal build system. It handles building artifacts, managing configuration, running built artifacts, and administering system services.

## Global Options

| Flag | Default | Description |
|------|---------|-------------|
| `--level <LEVEL>` | `INFO` | Log level (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`) |
| `--version` | | Print version information |
| `--help` | | Print help information |

Log output goes to stderr. At `DEBUG` and `TRACE` levels, file names and line numbers are included.

## `vorpal build`

Build an artifact by name from a Vorpal configuration file.

```bash
vorpal build <NAME> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<NAME>` | Name of the artifact to build (required) |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Agent service address |
| `--config <PATH>` | `Vorpal.toml` | Path to configuration file |
| `--context <PATH>` | `.` | Build context directory |
| `--export` | `false` | Export the artifact definition as JSON instead of building |
| `--namespace <NAME>` | `library` | Artifact namespace |
| `--path` | `false` | Print the output path instead of the digest |
| `--rebuild` | `false` | Force rebuild, ignoring cached outputs |
| `--registry <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Registry service address |
| `--system <SYSTEM>` | Host system | Target system (e.g., `aarch64-darwin`, `x86_64-linux`) |
| `--unlock` | `false` | Allow source digests to change (update lockfile) |
| `--variable <KEY=VALUE>` | | Set build variables (can be repeated) |
| `--worker <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Worker service address |

### Examples

```bash
# Build an artifact named "my-app"
vorpal build my-app

# Build with a custom config file
vorpal build my-app --config Vorpal.go.toml

# Force rebuild and print the output path
vorpal build my-app --rebuild --path

# Build for a specific target system
vorpal build my-app --system x86_64-linux

# Update locked source digests
vorpal build my-app --unlock

# Export artifact definition as JSON (no build)
vorpal build my-app --export

# Pass build variables
vorpal build my-app --variable VERSION=1.2.3 --variable ENV=prod
```

### Output

By default, prints the artifact's SHA-256 content digest. With `--path`, prints the filesystem path to the artifact output directory. With `--export`, prints the artifact definition as pretty-printed JSON.

## `vorpal config`

Manage project-level and user-level configuration settings.

```bash
vorpal config [--user] [--config <PATH>] <SUBCOMMAND>
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--user` | `false` | Apply to user-level config (`~/.vorpal/settings.json`) instead of project-level |
| `--config <PATH>` | `Vorpal.toml` | Path to the project-level configuration file |

### `vorpal config set`

Set a configuration value.

```bash
vorpal config set <KEY> <VALUE>
```

Valid keys: `registry`, `namespace`, `language`, `name`, `system`, `worker`.

```bash
# Set registry in project config
vorpal config set registry "https://registry.example.com:23151"

# Set namespace in user config
vorpal config --user set namespace "my-team"
```

### `vorpal config get`

Get a configuration value with its source.

```bash
vorpal config get <KEY>
```

```bash
$ vorpal config get registry
registry = unix:///var/lib/vorpal/vorpal.sock (default)
```

### `vorpal config show`

Show all configuration values with their sources.

```bash
$ vorpal config show
KEY        VALUE                                  SOURCE
---        -----                                  ------
registry   unix:///var/lib/vorpal/vorpal.sock     default
namespace  library                                default
language   rust                                   project
name       vorpal-config                          project
system     aarch64-darwin                         default
worker     unix:///var/lib/vorpal/vorpal.sock     default
```

## `vorpal init`

Scaffold a new Vorpal project with language selection.

```bash
vorpal init <NAME> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<NAME>` | Project name (required) |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--path <PATH>` | `.` | Output directory for the project files |

### Interactive Prompt

The command presents an interactive language selector:

- **Go** -- Generates `cmd/<name>/main.go`, `cmd/vorpal/main.go`, `go.mod`, `go.sum`, `Vorpal.toml`. The template directory `cmd/example/` is dynamically renamed to `cmd/<name>/` using the project name.
- **Rust** -- Generates `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/vorpal.rs`, `Vorpal.toml`, `.gitignore`
- **TypeScript** -- Generates `src/vorpal.ts`, `src/main.ts`, `package.json`, `tsconfig.json`, `Vorpal.toml`, `.gitignore`, `bun.lock`

```bash
# Create a new project in the current directory
vorpal init my-project

# Create a new project in a specific directory
vorpal init my-project --path /path/to/project
```

## `vorpal inspect`

Inspect a stored artifact by its content digest. Prints the full artifact definition as JSON.

```bash
vorpal inspect <DIGEST> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<DIGEST>` | Artifact SHA-256 content digest (required) |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--namespace <NAME>` | `library` | Artifact namespace |
| `--registry <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Registry service address |

```bash
# Inspect an artifact by digest
vorpal inspect abc123def456...
```

## `vorpal login`

Authenticate with an OAuth2 provider using the device authorization flow.

```bash
vorpal login [OPTIONS]
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--issuer <URL>` | `http://localhost:8080/realms/vorpal` | OIDC issuer base URL |
| `--issuer-audience <AUDIENCE>` | | OAuth2 audience parameter |
| `--issuer-client-id <ID>` | `cli` | OAuth2 client ID |
| `--registry <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Registry to associate credentials with |

The command performs OAuth2 device flow authentication, displays a verification URL and code, then stores the resulting credentials at `/var/lib/vorpal/key/credentials.json`.

```bash
# Login with default settings (local Keycloak)
vorpal login

# Login to a production identity provider
vorpal login --issuer https://id.example.com/realms/vorpal --registry https://registry.example.com:23151
```

## `vorpal prepare`

Prepare an artifact: download and pin its sources into `Vorpal.lock` without building the target artifact. Config-language toolchain prerequisites (e.g. protoc for Go configs) still build via the worker as needed to execute the config binary and enumerate the artifact graph — this always runs host-natively, so it works from any host for any `--system` target.

```bash
vorpal prepare <NAME> [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<NAME>` | Name of the artifact to prepare (required) |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Agent service address |
| `--config <PATH>` | `Vorpal.toml` | Path to configuration file |
| `--context <PATH>` | `.` | Build context directory |
| `--namespace <NAME>` | `library` | Artifact namespace |
| `--registry <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Registry service address |
| `--system <SYSTEM>` | Host system | Target system for the artifact (e.g., `aarch64-darwin`, `x86_64-linux`) |
| `--unlock` | `true` | Pin/update source digests in `Vorpal.lock`. Unlike `build` (where this defaults to `false`), `prepare` defaults to `true`, since minting and updating pins is its entire purpose |
| `--variable <KEY=VALUE>` | | Set build variables (can be repeated) |
| `--worker <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Worker service address |

`--unlock` accepts three forms: bare `--unlock` (equivalent to `--unlock=true`), `--unlock=false` to opt out and enforce the fail-closed pin gates, or omitting the flag entirely (defaults to `true`). The space-separated form `--unlock false` is rejected as an unexpected argument — use `--unlock=false`.

### Examples

```bash
# Prepare an artifact, minting or updating its source pins by default
vorpal prepare my-app

# Prepare non-interactively in CI without minting/updating pins (fails
# closed on any unpinned or changed source, same gates as `build`)
vorpal prepare my-app --unlock=false

# Prepare sources for a foreign target system from any host — e.g. a Linux
# CI runner preparing a darwin target, or an aarch64-darwin host preparing
# x86_64-darwin. This works because the config binary that enumerates the
# artifact graph always compiles and runs host-native regardless of the
# requested --system; only the artifact target and its downloaded/pinned
# sources vary by --system. No OS/arch matching or binary-translation layer
# is involved.
vorpal prepare my-app --system x86_64-darwin
```

### Output

Prints one sorted, deduplicated audit line per remote (HTTP/HTTPS) source across the artifact and its dependency closure, classifying each pin against its prior `Vorpal.lock` entry:

- `mint: <name> (<platform>) -> <digest>` — no prior lock entry (first-use trust decision)
- `update: <name> (<platform>) -> <digest>` — prior entry with a different digest (trust rotation)
- `verify: <name> (<platform>) -> <digest>` — prior entry with the same digest (unchanged)

After the audit lines, prints the selected artifact's SHA-256 content digest, matching `build`'s default output. `prepare` never dispatches a worker build for the target artifact or its dependencies — only `Vorpal.lock` is updated. Building the config binary itself (and its language toolchain, e.g. protoc for Go configs) still happens as usual, since the config binary must run to enumerate the artifact graph.

## `vorpal run`

Execute a previously built artifact by alias. The alias format is `[<namespace>/]<name>[:<tag>]`.

```bash
vorpal run <ALIAS> [OPTIONS] [-- ARGS...]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<ALIAS>` | Artifact alias (required). Format: `[namespace/]name[:tag]` |
| `[ARGS...]` | Arguments passed through to the artifact binary |

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--bin <NAME>` | Artifact name | Override the binary name to execute within the artifact |
| `--registry <ADDRESS>` | `unix:///var/lib/vorpal/vorpal.sock` | Registry service address |

The command resolves the alias to a digest (checking local store, then registry), locates the binary in the artifact's `bin/` directory, and replaces the current process with the binary (`exec`).

```bash
# Run an artifact
vorpal run my-tool

# Run with a specific binary from the artifact
vorpal run my-tools --bin specific-tool

# Run with arguments
vorpal run my-tool -- --verbose --output /tmp/out

# Run from a specific namespace and tag
vorpal run my-namespace/my-tool:v1.0
```

## `vorpal system`

Manage Vorpal system resources and services.

### `vorpal system keys generate`

Generate TLS key pairs for secure service communication. Creates CA certificate, service certificate, service keypair, and service secret in `/var/lib/vorpal/key/`.

```bash
vorpal system keys generate
```

Generated files:
- CA private key and certificate
- Service private key, public key, and certificate (signed by the CA)
- Service secret (UUID v7)

Files are only generated if they do not already exist. Safe to run multiple times.

### `vorpal system prune`

Clean up the local artifact store to free disk space.

```bash
vorpal system prune [OPTIONS]
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--all` | `false` | Prune all resource types |
| `--artifact-aliases` | `false` | Prune artifact alias references |
| `--artifact-archives` | `false` | Prune compressed artifact archives |
| `--artifact-configs` | `false` | Prune compiled configuration outputs |
| `--artifact-outputs` | `false` | Prune unpacked artifact outputs |
| `--sandboxes` | `false` | Prune build sandbox directories |

```bash
# Prune everything
vorpal system prune --all

# Prune only archives and outputs
vorpal system prune --artifact-archives --artifact-outputs

# Prune only sandboxes
vorpal system prune --sandboxes
```

### `vorpal system services start`

Start the gRPC backend services (agent, registry, worker).

```bash
vorpal system services start [OPTIONS]
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--archive-cache-ttl <SECONDS>` | `300` | TTL for caching archive check results (0 to disable) |
| `--health-check` | `false` | Enable plaintext gRPC health check endpoint |
| `--health-check-port <PORT>` | `23152` | Port for health check listener |
| `--issuer <URL>` | | OIDC issuer URL for JWT validation |
| `--issuer-audience <AUDIENCE>` | | Expected JWT audience |
| `--issuer-client-id <ID>` | | OAuth2 client ID for worker-to-registry auth |
| `--issuer-client-secret <SECRET>` | | OAuth2 client secret for worker-to-registry auth |
| `--port <PORT>` | | TCP port (omit for Unix domain socket mode) |
| `--registry-backend <BACKEND>` | `local` | Registry storage backend (`local` or `s3`) |
| `--registry-backend-s3-bucket <BUCKET>` | | S3 bucket name (required when backend is `s3`) |
| `--registry-backend-s3-force-path-style` | `false` | Use path-style S3 URLs |
| `--services <LIST>` | `agent,registry,worker` | Comma-separated list of services to start |
| `--tls` | `false` | Enable TLS (requires keys from `vorpal system keys generate`) |

### Transport Modes

- **Unix domain socket** (default): Listens on `/var/lib/vorpal/vorpal.sock`. Override with `VORPAL_SOCKET_PATH` environment variable.
- **TCP**: When `--port` is specified. Listens on `[::]:<port>`.
- **TLS over TCP**: When `--tls` is enabled. Defaults to port 23151 if `--port` is not specified.

```bash
# Start all services with defaults (Unix socket)
vorpal system services start

# Start with TLS enabled
vorpal system services start --tls

# Start on a specific TCP port
vorpal system services start --port 23151

# Start with S3 registry backend
vorpal system services start --registry-backend s3 --registry-backend-s3-bucket my-bucket

# Start only the registry service
vorpal system services start --services registry

# Enable health checks
vorpal system services start --health-check

# Start with OIDC authentication
vorpal system services start \
  --issuer https://id.example.com/realms/vorpal \
  --issuer-audience vorpal \
  --issuer-client-id worker \
  --issuer-client-secret <secret>
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `VORPAL_SOCKET_PATH` | Override the default Unix domain socket path (`/var/lib/vorpal/vorpal.sock`) |

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (build failure, missing artifact, configuration error, etc.) |
