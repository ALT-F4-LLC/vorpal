# vorpal

Build and deliver software reliably with one magical tool.

## Overview

Vorpal's goal is to package and distribute software reliably to local (development) and remote (cloud, self-hosted, etc) environments. It uses a `vorpal.ncl` file written in [Nickel](https://nickel-lang.org/) that allows you to "describe" every aspect of your software dependencies in a repeatable and reproducible way.

```nickel
# Built-in validation contracts
let { Config, .. } = import "schema.ncl" in

# Built-in language functions
let { RustPackage, .. } = import "language.ncl" in

# Project configuration (with `--system "<system>"` value)
fun system => {
  packages = {
    default = RustPackage {
      cargo_hash = "<hash>",
      name = "vorpal",
      source = ".",
      systems = ["aarch64-linux", "x86_64-linux"],
      target = system
    }
  }
} | Config
```

## Design

Below is the existing working diagram that illustrates the platform's design:

> [!CAUTION]
> This design is subject to change at ANY moment and is a work in progress.

![vorpal](./vorpal.png)

## Development

### Requirements

The following tools are required to develop:

- [`curl`](https://curl.se) (http client)
- [`direnv`](https://direnv.net) (environment variables)
- [`rustup`](https://rustup.rs) (language toolchains)

### Steps

The following steps guide how to setup and run commands in the development environment.

> [!NOTE]
> Steps must be run in the root of the cloned repository.

- Run `dev.sh` script to bootstrap dependencies:

```bash
./dev.sh
```

- Run to enter development environment:

```bash
direnv allow
```

- Build the source code with:

```bash
cargo build
```
