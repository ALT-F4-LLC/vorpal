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

Development environments require these tools for the best experience:

- `just` commands
- `nix` builder
- `nix-direnv` environment

### Building

Building the project is managed by `just` and `nix`. Here are steps to building the project locally:

1. Ensure you have `just` and `nix` installed on your system (see more on these tools below)
2. Navigate to Vorpal's project root directory
3. Enter the development environment with `nix develop` or `nix-direnv`

At this point you should be able to use `just` in the shell and can run one of the following:

- `just build` which uses Cargo for faster builds
- `just package` which uses Nix for reproducible builds

```bash
$ just build # faster builds
$ just package # reproducible builds
```

> [!NOTE]
> Builds with `nix` may take longer as it doesn't save any build cache like Cargo does. This is why the `just build` command is suggested after entering the `nix` development shell.

### Running

Here are the steps to run the project:

1. Navigate to the project root directory.
2. Run `cargo run` with the CLI with `vorpal-cli` as the package:

```bash
cargo run --package "vorpal-cli" -- --help

Usage: vorpal <COMMAND>

Commands:
  build
  keys
  validate
  worker
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

This will display a list of available commands and options for running the project.

You can also run specific commands by appending them to the `vorpal` binary. For example, to start the server, you might use:

```bash
$ ./vorpal keys generate # create signing keys
$ ./vorpal worker start # start worker
```

Or, if you'd like to start all services with `nix` run:

```bash
$ just start
```

### Tools

#### just

Just runs all dev and CI commands used working with Vorpal source. To display all available commands run `just` or `just --list`:

> [!TIP]
> This [video](https://www.youtube.com/watch?v=2CKX2epvAtY) explains more why `just` was chosen.

```
$ just --list

Available recipes:
    build                     # build everything
    build-sandbox tag="edge"  # build sandbox (only)
    check                     # check everything
    check-cargo               # check cargo
    check-nix                 # check nix
    clean                     # clean everything
    format                    # format everything
    format-cargo              # format cargo
    format-nix                # format nix
    lint                      # lint
    package profile="default" # package (nix)
    start                     # start (worker)
    test                      # test everything
    test-cargo                # test cargo
    test-nickel               # test nickel
    update                    # update everything
    update-cargo              # update cargo
    update-nix                # update nix
```

#### nix

Until we replace Nix with Vorpal (coming soon), Nix is used to manage all dependencies and create a consistent development environment.

- To build the project using Nix, you can use the command `just package`.
- To enter a development shell with all dependencies available, use `nix-develop` or other tools like `nix-direnv`

#### nix-direnv

`nix-direnv` is a tool that allows you to use `direnv` with `nix` to automatically enter a development shell when you change into a project directory.

> [!TIP]
> This means you don't have to manually run `nix-shell` or `nix-develop` every time you start working on the project.

To use `nix-direnv`:

1. Install `direnv` and `nix-direnv`
2. Run `direnv allow` in Vorpal's project root

Now, every time you change into your project directory, `direnv` will automatically load the development environment specified by `nix`.
