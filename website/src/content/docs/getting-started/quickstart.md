---
title: Quickstart
description: Create, build, and run your first Vorpal project in minutes.
---

This guide walks you through creating a new Vorpal project, building an artifact, and running it. You should have Vorpal [installed](./installation) before continuing.

## 1. Create a project

Create a new directory and initialize a Vorpal project:

```bash
mkdir hello-world && cd hello-world
vorpal init hello-world
```

The `init` command prompts you to choose a language for your build configuration -- Go, Rust, or TypeScript. Vorpal scaffolds a working project with a `Vorpal.toml` manifest and a sample build configuration in your chosen language.

Your project structure will look something like this:

```
hello-world/
  Vorpal.toml          # Project manifest
  src/                 # Build configuration source
```

## 2. Build

Compile your build configuration and produce a content-addressed artifact:

```bash
vorpal build hello-world
```

On the first build, Vorpal downloads the necessary toolchains for your target platforms. Subsequent builds use the local cache -- unchanged inputs resolve instantly without re-downloading or re-compiling.

## 3. Run

Execute your built artifact:

```bash
vorpal run hello-world
```

That is it. Your artifact is built, cached, and runnable.

## 4. Develop

All `vorpal init` projects include a language-specific development environment with relevant tooling out of the box. Activate it:

```bash
source $(vorpal build --path hello-world-shell)/bin/activate
```

This drops you into a shell with your project's dependencies available -- consistent across machines and teammates. To exit the environment, run `deactivate` or close the shell.

See [Environments](/concepts/environments/) to learn more about development and user environments.

## What just happened

When you ran `vorpal build`, Vorpal:

1. **Compiled your build configuration** -- your config is a real program (not YAML or a DSL), so Vorpal compiles and executes it to determine what to build.
2. **Resolved dependencies** -- any toolchains or artifacts your build depends on are fetched and cached.
3. **Produced a content-addressed artifact** -- the output is stored by its content hash, so identical inputs always produce the same output.

When you ran `vorpal run`, Vorpal looked up the artifact by name and executed it.

## Next steps

Now that you have a working project, explore the SDK guide for your language:

- [Rust guide](../guides/rust)
- [Go guide](../guides/go)
- [TypeScript guide](../guides/typescript)

To understand how Vorpal's build model works under the hood, see the [Architecture](../concepts/architecture) overview.
