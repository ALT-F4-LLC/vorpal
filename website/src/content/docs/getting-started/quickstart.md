---
title: Quickstart
description: Create, develop, build, and run your first Vorpal project in minutes.
---

This guide walks you through creating a Vorpal project, setting up a development environment, building an artifact, and running it. You should have Vorpal [installed](/getting-started/installation/) before continuing.

## 1. Create your project

Create a new directory and initialize a Vorpal project:

```bash
mkdir hello-world && cd hello-world
vorpal init hello-world
```

The `init` command prompts you to choose a language for your build configuration -- Go, Rust, or TypeScript. Vorpal scaffolds a working project with a `Vorpal.toml` manifest and a sample build configuration in your chosen language.

## 2. Develop your project

All `vorpal init` projects include a language-specific development environment with the relevant toolchain pinned and ready. Activate it:

```bash
source $(vorpal build --path hello-world-shell)/bin/activate
```

This drops you into a shell with your project's dependencies available -- consistent across machines and teammates. To exit the environment, run `deactivate` or close the shell.

See [Environments](/concepts/environments/) to learn more about development and user environments.

## 3. Build your project

Compile your build configuration and produce a content-addressed artifact:

```bash
vorpal build hello-world
```

On the first build, Vorpal downloads the necessary toolchains for your target platforms. Subsequent builds use the local cache -- unchanged inputs resolve instantly without re-downloading or re-compiling.

A successful build returns the artifact's content digest. To get the full filesystem path to the artifact instead, use the `--path` flag:

```bash
vorpal build --path hello-world
```

To force a fresh build and skip the cache, use `--rebuild`:

```bash
vorpal build --rebuild hello-world
```

See [`vorpal build`](/reference/cli/#vorpal-build) in the CLI reference for all available options.

## 4. Access your artifact output

Using the `--path` flag from the previous step, you can inspect the artifact's contents with standard tools:

```bash
ls $(vorpal build --path hello-world)
```

:::caution
The Vorpal store is immutable. Do not modify, add, or remove files in the artifact output directory outside of the Vorpal build process.
:::

This is useful for inspecting build output, copying files into a deployment pipeline, or verifying that your artifact contains the expected structure.

## Next steps

Now that you have a working project, explore the SDK guide for your language:

- [Go guide](/guides/go/)
- [Rust guide](/guides/rust/)
- [TypeScript guide](/guides/typescript/)

To understand how Vorpal's build model works under the hood, see the [Architecture](/concepts/architecture/) overview.
