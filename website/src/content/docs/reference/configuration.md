---
title: Configuration
description: Complete reference for Vorpal configuration files, keys, and resolution.
---

Vorpal uses a layered configuration system. Settings are resolved from three sources in order of precedence (highest to lowest):

1. **CLI flags** -- Explicit flags on the command line
2. **Project config** -- `Vorpal.toml` in the project root
3. **User config** -- `~/.vorpal/settings.json`
4. **Built-in defaults**

## Project Configuration (`Vorpal.toml`)

The `Vorpal.toml` file defines build configuration for a project. It is a TOML file with settings keys at the top level and build-specific sections nested under `[source]`.

### Settings Keys

These keys control how the CLI connects to services and resolves builds.

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `language` | string | `"rust"` | Build configuration language (`rust`, `go`, or `typescript`) |
| `name` | string | `"vorpal"` | Name of the configuration binary to build |
| `namespace` | string | `"library"` | Artifact namespace for storage and isolation |
| `registry` | string | `"unix:///var/lib/vorpal/vorpal.sock"` | Registry service address |
| `system` | string | Host system | Target build system (e.g., `aarch64-darwin`) |
| `worker` | string | `"unix:///var/lib/vorpal/vorpal.sock"` | Worker service address |

### `[source]` Section

Controls which files are included in the build configuration source.

| Key | Type | Description |
|-----|------|-------------|
| `includes` | string[] | List of file paths or directories to include as source input |
| `script` | string | Optional pre-build script to run on the source |

### `[source.rust]` Section

Rust-specific source configuration.

| Key | Type | Description |
|-----|------|-------------|
| `bin` | string | Override the binary name produced by the Rust build |
| `packages` | string[] | Cargo packages to include in the build |

### `[source.go]` Section

Go-specific source configuration.

| Key | Type | Description |
|-----|------|-------------|
| `directory` | string | Go module directory containing `main.go` |

### `[source.typescript]` Section

TypeScript-specific source configuration.

| Key | Type | Description |
|-----|------|-------------|
| `entrypoint` | string | TypeScript entrypoint file (default: `src/<name>.ts`) |
| `directory` | string | Working directory for the TypeScript build |

### `environments` Key

| Key | Type | Description |
|-----|------|-------------|
| `environments` | string[] | Environment variables (`KEY=VALUE`) passed to the build |

### Examples

#### Rust Configuration

```toml
language = "rust"
name = "vorpal-config"

[source]
includes = [
    "config",
    "sdk/rust",
]

[source.rust]
packages = [
    "vorpal-config",
    "vorpal-sdk",
]
```

#### Go Configuration

```toml
language = "go"

[source]
includes = ["sdk/go"]

[source.go]
directory = "sdk/go/cmd/vorpal"
```

#### TypeScript Configuration

```toml
language = "typescript"

[source]
includes = [
    "sdk/typescript/src",
    "sdk/typescript/bun.lock",
    "sdk/typescript/package.json",
    "sdk/typescript/tsconfig.json"
]

[source.typescript]
directory = "sdk/typescript"
entrypoint = "src/vorpal.ts"
```

## User Configuration (`~/.vorpal/settings.json`)

User-level settings are stored as JSON at `~/.vorpal/settings.json`. These provide defaults that apply across all projects for the current user.

Override the config directory with the `VORPAL_USER_CONFIG_DIR` environment variable.

### Format

```json
{
  "registry": "https://registry.example.com:23151",
  "namespace": "my-team",
  "worker": "https://worker.example.com:23151"
}
```

All fields are optional. Only set fields override the built-in defaults.

### Managing User Config

```bash
# Set a user-level value
vorpal config --user set registry "https://registry.example.com:23151"

# Get a value (shows resolved source)
vorpal config get registry

# Show all values with sources
vorpal config show
```

## Lockfile (`Vorpal.lock`)

The lockfile pins source digests per platform to ensure reproducible builds. It is automatically created and updated during builds.

### Format

```toml
lockfile = 1

[[sources]]
name = "source-name"
digest = "sha256-hex-digest"
platform = "aarch64-darwin"
path = "https://example.com/archive.tar.gz"
includes = []
excludes = []
```

### Behavior

- Sources are locked after first resolution
- Locked sources cannot change without the `--unlock` flag
- Each source entry is platform-specific
- The lockfile should be committed to version control

```bash
# Build normally (locked mode - rejects changed sources)
vorpal build my-app

# Update locked sources
vorpal build my-app --unlock
```

## Storage Layout

Vorpal stores all data under `/var/lib/vorpal/`:

| Path | Purpose |
|------|---------|
| `vorpal.sock` | Unix domain socket for service communication |
| `vorpal.lock` | Advisory lock file preventing concurrent server instances |
| `key/` | TLS certificates, keypairs, and credentials |
| `store/artifact/alias/` | Named references mapping aliases to artifact digests |
| `store/artifact/archive/` | Compressed (zstd) artifact archives |
| `store/artifact/config/` | Compiled configuration outputs |
| `store/artifact/output/` | Unpacked artifact outputs |
| `sandbox/` | Isolated build workspaces |
| `log/` | Service log files |

## Supported Systems

The `system` configuration value accepts these platform identifiers:

| Value | Platform |
|-------|----------|
| `aarch64-darwin` | macOS Apple Silicon |
| `x86_64-darwin` | macOS Intel |
| `aarch64-linux` | Linux ARM64 |
| `x86_64-linux` | Linux x86_64 |
