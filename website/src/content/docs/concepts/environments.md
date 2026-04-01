---
title: Environments
description: How Vorpal creates reproducible development and user environments with pinned dependencies.
---

Vorpal provides two types of managed environments: **development environments** for project-scoped tooling and **user environments** for machine-wide tool installations. Both are built on the same content-addressed artifact system, which means they are reproducible, versioned, and shareable.

## The problem with system-installed tools

Traditional development setups rely on tools installed globally on the developer's machine -- via Homebrew, apt, or manual downloads. This creates several issues:

- **Version drift** -- Different team members may have different versions of the same tool, leading to subtle "works on my machine" bugs.
- **Implicit dependencies** -- Build scripts assume tools exist at specific paths, but nothing enforces this.
- **Upgrade risk** -- Upgrading a system tool for one project can break another project that depends on the older version.

Vorpal environments solve these problems by treating tool installations as artifacts: defined in code, version-pinned by content hash, and isolated from the host system.

## Development environments

A development environment creates a project-scoped shell with a specific set of tools and environment variables. It is similar in concept to Python's virtualenv or tools like direnv, but it works across languages and manages binary tools, not just language packages.

You define a development environment in your build configuration:

```typescript
import {
  ConfigContext,
  ArtifactSystem,
  DevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new DevelopmentEnvironment("my-project", SYSTEMS)
  .withEnvironments(["FOO=bar", "NODE_ENV=development"])
  .build(context);

await context.run();
```

When built, Vorpal produces an artifact whose output contains an activation script. To enter the environment:

```bash
source <artifact-output>/bin/activate
```

While activated, the environment provides:

- **Pinned tools** available on `PATH` -- the exact versions specified in your configuration
- **Custom environment variables** -- set through `.withEnvironments()`
- **Isolation from the host** -- tools outside the environment are not affected

Because development environments are artifacts, they benefit from the same [caching](./caching) as any other build output. If a teammate has already built the environment, you pull it from the cache instead of rebuilding.

## User environments

While development environments are project-scoped (activated per shell session), user environments install tools and configurations persistently under `~/.vorpal/`. They are intended for tools you want available everywhere, not tied to a specific project.

```typescript
import {
  ConfigContext,
  ArtifactSystem,
  UserEnvironment,
} from "@altf4llc/vorpal-sdk";

const SYSTEMS = [
  ArtifactSystem.AARCH64_DARWIN,
  ArtifactSystem.X8664_LINUX,
];

const context = ConfigContext.create();

await new UserEnvironment("my-tools", SYSTEMS)
  .withSymlinks([
    ["/path/to/artifact/bin/tool", "$HOME/.vorpal/bin/tool"],
  ])
  .build(context);

await context.run();
```

User environments work by creating symlinks from artifact outputs into well-known paths (such as `~/.vorpal/bin/`). To activate:

```bash
$HOME/.vorpal/bin/vorpal-activate
source $HOME/.vorpal/bin/vorpal-activate-shell
```

The `vorpal-activate` script manages symlinks -- removing any from a previous activation and creating the new ones. The `vorpal-activate-shell` script adds the artifact output directories to your `PATH`, making the environment's tools available in the current shell session.

## How environments differ from containers

Vorpal environments are not containers. They do not provide filesystem or process isolation. Instead, they modify the shell environment (primarily `PATH` and environment variables) to make pinned tools available alongside the host system's tools.

This is a deliberate design choice:

- **Lower overhead** -- No container runtime, no image layers, no volume mounts. Environments activate in milliseconds.
- **Host integration** -- You can use host tools (editors, debuggers, system utilities) alongside environment-managed tools without workarounds.
- **Simplicity** -- There is no container networking, no filesystem mapping, and no Docker dependency.

The tradeoff is weaker isolation. If you need strict filesystem or network isolation for your build steps, consider using Docker or Bubblewrap as the entrypoint for your [artifact](./artifacts) build steps rather than environments.

## Cross-platform environments

Like all Vorpal artifacts, environments declare their target platforms. The SDK builders automatically select the correct tool binaries for the host architecture. A single environment definition can target macOS (Apple Silicon and Intel) and Linux (x86_64 and ARM64) -- Vorpal resolves the right binaries at build time.

This means you can share an environment definition across a team with mixed hardware. Each developer builds the environment for their platform, and the content-addressed cache stores separate outputs per platform.

## Reproducibility

Because environments are built through the same content-addressed pipeline as any other artifact:

- **The same config always produces the same environment.** If two developers run the same build configuration, they get identical tool versions.
- **Environments are versioned by their inputs.** Changing a tool version or adding an environment variable changes the content digest, producing a new environment. The old one remains cached if you need to roll back.
- **Environments are shareable.** Push a built environment to the Registry, and teammates can pull it instead of rebuilding. See [Caching](./caching) for how this works.
