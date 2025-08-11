# vorpal

Build and ship software with one powerful tool.

<p align="center">
  <img src="./vorpal-purpose.jpg" />
</p>

## Overview

Vorpal uses declarative "bring-your-own-language" configurations to build software distributively and natively in a repeatable and reproducible way.

Examples of building a Rust application in multiple languages:

## Make Targets and Modes

- `make vorpal`: Ensure mode (reproduce from `Vorpal.lock`; does not modify lock). Fails fast if the lock would change and suggests `make vorpal-update`.
- `make vorpal-update`: Update mode (re-resolve remote sources and write `Vorpal.lock`).
- `make vorpal-offline`: Offline ensure (no network; uses only local cache; fails if any locked digest is missing locally).
- `make vorpal-verify`: Verify lock (checks remote digests exist in the registry).

Notes:
- Remote/toolchain sources are prepared once and then referenced by digest from the lockfile.
- Local sources are dynamic per run and are not tracked in `Vorpal.lock`.

## Install

```
curl -fsSL https://github.com/ALT-F4-LLC/vorpal/blob/main/script/install.sh -o install.sh
sh install.sh
```

### Rust

```rust
use anyhow::Result;
use vorpal_sdk::{
    api::artifact::{
        ArtifactSystem,
        ArtifactSystem::{Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux},
    },
    artifact::language::rust::RustArtifactBuilder,
    context::get_context,
};

const SYSTEMS: [ArtifactSystem; 4] = [Aarch64Darwin, Aarch64Linux, X8664Darwin, X8664Linux];

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Get context
    let context = &mut get_context().await?;

    // 2. Create artifact
    RustArtifactBuilder::new("example", SYSTEMS.to_vec())
        .build(context)
        .await?;

    // 3. Run context with artifacts
    context.run().await
}
```

### Go

```go
package main

import (
    "log"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

var SYSTEMS = []artifact.ArtifactSystem{
	artifact.ArtifactSystem_AARCH64_DARWIN,
	artifact.ArtifactSystem_AARCH64_LINUX,
	artifact.ArtifactSystem_X8664_DARWIN,
	artifact.ArtifactSystem_X8664_LINUX,
}

func main() {
    // 1. Get context
    context := config.GetContext()

    // 2. Create artifact
    _, err := language.
        NewRustBuilder("example", SYSTEMS).
        Build(context)
    if err != nil {
        log.Fatalf("failed to build artifact: %v", err)
    }

    // 3. Run context with artifacts
    context.Run()
}
```

### Python

```python
from vorpal_sdk.config import get_context
from vorpal_sdk.api.artifact import ArtifactSystem
from vorpal_sdk.config.artifact.language.rust import RustArtifactBuilder

SYSTEMS = [
    ArtifactSystem.AARCH64_DARWIN,
    ArtifactSystem.AARCH64_LINUX,
    ArtifactSystem.X8664_DARWIN,
    ArtifactSystem.X8664_LINUX,
]

def main():
    # 1. Get context
    context = get_context()

    # 2. Create artifact
    RustArtifactBuilder("example", SYSTEMS).build(context)

    # 3. Run context with artifacts
    context.run()

if __name__ == "__main__":
    main()
```

### TypeScript

```typescript
import { getContext } from '@vorpal/sdk';
import { ArtifactSystem } from '@vorpal/sdk/api/artifact';
import { RustArtifactBuilder } from '@vorpal/sdk/config/artifact/language/rust';

const SYSTEMS = [
    ArtifactSystem.AARCH64_DARWIN,
    ArtifactSystem.AARCH64_LINUX,
    ArtifactSystem.X8664_DARWIN,
    ArtifactSystem.X8664_LINUX,
];

async function main() {
    // 1. Get context
    const context = await getContext();

    // 2. Create artifact
    await new RustArtifactBuilder('example', SYSTEMS)
        .build(context);

    // 3. Run context with artifacts
    await context.run();
}

main().catch(console.error);
```

## Components

Below is the existing working diagram that illustrates the platform's design:

> [!CAUTION]
> This design is subject to change at ANY moment and is a work in progress.

![vorpal-domains](./vorpal-domains.svg)

## Artifacts

Vorpal uses `artifacts` to describe every aspect of your software in the language of your choice:

```rust
Artifact {
    // required: name of artifact
    name: "example".to_string(),

    // optional: named aliases
    aliases: vec![],

    // optional: source paths for artifact
    sources: vec![
        ArtifactSource {
            name: "example", // required: unique per source
            path: ".", // required: relative location to context
            excludes: vec![], // optional: to remove files
            hash: None, // optional: to track changes
            includes: vec![], // optional: to only use files
        }
    ],

    // required: steps of artifact (in order)
    steps: vec![
        ArtifactStep {
            entrypoint: Some("/bin/bash"), // required, host path for command (can be artifact)
            arguments: vec![], // optional, arguments for entrypoint
            artifacts: vec![], // optional, artifacts included in step
            environments: vec![], // optional, environment variables for step
            secrets: vec![], // optional, secrets to be added to environment
            script: Some("echo \"hello, world!\" > $VORPAL_OUTPUT/hello_world.txt"), // optional, script passed to executor
        },
    ],

    // systems for artifact
    systems: vec![Aarch64Darwin, Aarch64Linux],

    // target
    target: Aarch64Darwin
};
```

Artifacts can be wrapped in language functions and/or modules to be shared within projects or organizations providing centrally managed and reusable configurations with domain-specific overrides (see examples in overview).

### Sources

Coming soon.

### Steps

Steps provided by the SDKs are maintained to provide reproducibile cross-platform environments for them. These environments include strictly maintained low-level dependencies that are used as a wrapper for each step.

> [!NOTE]
> Vorpal enables developers to create their own build steps instead of using the SDKs which are provided to handle "common" scenarios.

#### Linux

On Linux, developers can run steps in a community maintained sandbox which is isolated similiar to containers.

The following are included in the sandbox:

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

#### macOS

Coming soon.

#### Windows

Coming soon.

### Systems

Coming soon.

## Development

### Requirements

#### macOS

On macOS, install the native tools with Xcode:

```bash
xcode-select --install
```

#### Linux

On Linux, install dependencies with the distro's package manger (apt, yum, etc):

> [!IMPORTANT]
> If you are using NixOS, there is a `shell.nix` configuration included for the development environment.

- `bubblewrap` (sandboxing)
- `curl` (downloading)
- `docker` (sandboxing)
- `protoc` (compiling)
- `unzip` (downloading)

The helpful `./script/debian.sh` used for setting up systems in continuous integration can also be used to setup any similiar Debian-based systems.

### Setup

The helpful `./script/dev.sh` used to run development commands in an isolated way without having to update your environment. 

> [!IMPORTANT]
> If you are using NixOS, there is a `shell.nix` configuration included for the development environment.

The following installs missing dependencies then runs `cargo build` inside the development environment:

```bash
$ ./script/dev.sh cargo build
```

#### Direnv

To develop inside the environment the supported solution is to use `direnv` which manages all of this for you. Direnv will automatically run "./script/dev.sh" under the hood and export environment variables to your shell when you enter the directory.

Once you've installed `direnv` on your system navigate to Vorpal's source code and run:

```bash
$ direnv allow
```

### Testing

At this point, you should be able to run `cargo build` successfully in the repository. If that doesn't work, go back to "Setup" and verify you have done all the required steps.

These steps guide how to compile from source and also test compiling Vorpal with Vorpal.

1. Build without Vorpal:

```bash
make build
```

2. Run the initial install script, which will create all relevant directories and permissions needed to run the next steps.

> [!CAUTION]
> This step requires access to protected paths on your host filesystem. As such,
> it will likely require `sudo` privileges (or your system's equivalent) to run.

```bash
bash ./script/install.sh
```

3. Generate keys for Vorpal:

```bash
./target/debug/vorpal system keys generate
```

4. Start services for Vorpal:

```bash
./target/debug/vorpal start
```

5. Build with Vorpal:

```bash
./target/debug/vorpal artifact make "vorpal"
```

The entire stack of has now been tested by building itself.

### Makefile

There is makefile which can be used as a reference for common commands used when developing.

Here are some frequently used:

- `make` (default build)
- `make lint` (before pushing)
- `make dist` (package in `./dist` path)
- `make vorpal-start` (runs services with `cargo`)
- `make vorpal` (builds vorpal-in-vorpal with `cargo`)
