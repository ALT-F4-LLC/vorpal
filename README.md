# vorpal

Build and ship software reliably with one powerful tool.

![vorpal-purpose](./vorpal-purpose.jpg)

## Overview

Vorpal distributively builds and ships software reliably using BYOL (bring-you-own-language) programmable configurations. This allows developers to manage software dependencies and deployments in a repeatable and reproducible way.

Below are examples of building a Rust application with different configuration languages:

### Go

```go
package main

import (
    "context"
    "github.com/vorpal_schema/vorpal/config/v0"
    "github.com/vorpal_sdk/config/artifact"
    "github.com/vorpal_sdk/config/cli"
)

// 1. Create a function that returns a populated configuration
func config(ctx *cli.ContextConfig) (*config.Config, error) {
    // NOTE: custom logic can be added anywhere in this function

    // 2. Define artifact parameters
    artifactExcludes := []string{".env", ".packer", ".vagrant", "script"}
    artifactName := "vorpal"
    artifactSystems := artifact.AddSystems([]string{"aarch64-linux", "aarch64-macos"})

    // 3. Create artifact (rust)
    artifact, err := artifact.RustArtifact(ctx, artifactExcludes, artifactName, artifactSystems)
    if err != nil {
        return nil, err
    }

    // 4. Return config with artifact
    return &config.Config{
        Artifacts: []config.Artifact{artifact},
    }, nil
}

func main() {
    ctx := context.Background()
    if err := cli.Execute(ctx, config); err != nil {
        panic(err)
    }
}
```

### Rust

```rust
use anyhow::Result;
use vorpal_schema::vorpal::config::v0::Config;
use vorpal_sdk::config::{
    artifact::{add_systems, language::rust},
    cli::execute,
    ContextConfig,
};

// 1. Create a function that returns a populated configuration
fn config(context: &mut ContextConfig) -> Result<Config> {
    // NOTE: custom logic can be added anywhere in this function

    // 2. Define artifact parameters
    let artifact_excludes = vec![".env", ".packer", ".vagrant", "script"];
    let artifact_name = "vorpal";
    let artifact_systems = add_systems(vec!["aarch64-linux", "aarch64-macos"])?;

    // 3. Create artifact (rust)
    let artifact = rust::artifact(context, artifact_excludes, artifact_name, artifact_systems)?;

    // 4. Return config with artifact
    Ok(Config {
        artifacts: vec![artifact],
    })
}

#[tokio::main]
async fn main() -> Result<()> {
    // 5. Execute the configuration
    execute(config).await
}
```

### TypeScript

```typescript
import { ContextConfig, execute, addSystems, rust } from '@vorpal/config';
import { Config } from '@vorpal/schema';

// 1. Create a function that returns a populated configuration
function config(context: ContextConfig): Config {
    // NOTE: custom logic can be added anywhere in this function

    // 2. Define artifact parameters
    const artifactExcludes = ['.env', '.packer', '.vagrant', 'script'];
    const artifactName = 'vorpal';
    const artifactSystems = addSystems(['aarch64-linux', 'aarch64-macos']);

    // 3. Create artifact (rust)
    const artifact = rust.artifact(context, artifactExcludes, artifactName, artifactSystems);

    // 4. Return config with artifact
    return {
        artifacts: [artifact],
    };
}

// 5. Execute the configuration
await execute(config);
```

## Design

Below is the existing working diagram that illustrates the platform's design:

> [!CAUTION]
> This design is subject to change at ANY moment and is a work in progress.

![vorpal-arch](./vorpal-arch.png)

## Development

### Requirements

The following requirements are necessary to develop source code and dependant on the operating system.

#### macOS

On macOS, install native tools with Xcode:

```bash
xcode-select --install
```

#### Linux

On Linux, install native tools with the distro's package manger (apt, yum, etc):

> [!NOTE]
> Most tools below are used to compile packages for the sandbox environment.

- `bubblewrap`
- `curl`
- `docker`
- `protoc`
- `rustup`
- `unzip`

### Direnv

You can use `direnv` to load environment variables, `rustup` and `protoc`.

### Steps

These steps guide how to compile from source code and test Vorpal by building it with itself.

> [!IMPORTANT]
> Steps must be run in the root of the cloned repository.

1. Compile binaries:

```bash
./script/dev.sh make dist
```

2. Generate keys:

```bash
./dist/vorpal keys generate
```

3. Start services:

```bash
./dist/vorpal start
```

4. Check configuration:

```bash
./dist/vorpal config
```

5. Build artifacts:
```bash
./dist/vorpal build
```

## Sandboxes

Offical sandboxes maintained by the Vorpal development team that provide reproducibile environments.

### Linux

- `bash`
- `binutils`
- `bison`
- `coreutils`
- `curl`
- `diffutils`
- `file`
- `findutils`
- `gawk`
- `gcc`
- `gettext`
- `glibc`
- `grep`
- `gzip`
- `libidn2`
- `libpsl`
- `libunistring`
- `linux-headers`
- `m4`
- `make`
- `ncurses`
- `openssl`
- `patch`
- `perl`
- `python`
- `sed`
- `tar`
- `texinfo`
- `unzip`
- `util-linux`
- `xz`
- `zlib`

### macOS

Coming soon.

### Windows

Coming soon.
