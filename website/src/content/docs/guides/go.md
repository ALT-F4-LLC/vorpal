---
title: Go SDK
description: Build artifacts and environments with the Vorpal Go SDK.
---

The Go SDK lets you define Vorpal build configurations as Go programs. Your config compiles to a binary that communicates with the Vorpal daemon over gRPC.

## Installation

Add the SDK module to your Go project:

```bash
go get github.com/ALT-F4-LLC/vorpal/sdk/go
```

## Project setup

Create a build configuration in `main.go`:

```go title="main.go"
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    // Define your artifacts here

    ctx.Run()
}
```

Every Vorpal config starts by creating a context and defining target systems. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

### Build a Go project

Use the `Go` builder from `language` package to compile a Go project into a cross-platform artifact:

```go title="main.go" {5,20-25}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    _, err := language.NewGo("my-app", systems).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd", "go.mod", "go.sum"}).
        Build(ctx)
    if err != nil {
        log.Fatalf("error building: %v", err)
    }

    ctx.Run()
}
```

The `Go` builder:
- **`WithBuildDirectory`** — Sets the directory containing the `main` package
- **`WithIncludes`** — Lists files and directories to include in the build source

### Development environments

Create a portable development shell with pinned tools and environment variables:

```go title="main.go" {5,18-22}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    artifact.NewDevelopmentEnvironment("my-project", systems).
        WithEnvironments([]string{"FOO=bar"}).
        Build(ctx)

    ctx.Run()
}
```

Activate the environment by sourcing the generated `bin/activate` script inside the artifact output.

### User environments

Install tools into your user-wide environment with symlinks:

```go title="main.go" {5,18-20}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    artifact.NewUserEnvironment("my-home", systems).
        WithSymlinks(map[string]string{"/path/to/local/bin/app": "$HOME/.vorpal/bin/app"}).
        Build(ctx)

    ctx.Run()
}
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```go title="main.go" {5,18-28}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

var systems = []api.ArtifactSystem{
    api.ArtifactSystem_AARCH64_DARWIN,
    api.ArtifactSystem_AARCH64_LINUX,
    api.ArtifactSystem_X8664_DARWIN,
    api.ArtifactSystem_X8664_LINUX,
}

func main() {
    ctx := config.GetContext()

    step, _ := artifact.NewArtifactStep().
        WithEntrypoint("docker", systems).
        WithArguments([]string{
            "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
            "alpine", "sh", "-lc",
            "echo hi > /out/hi.txt",
        }, systems).
        Build(ctx)

    artifact.NewArtifact("example-docker",
        []*api.ArtifactStep{step}, systems).Build(ctx)

    ctx.Run()
}
```

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.

## Common patterns

### Builder options

The `Go` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithBuildDirectory(dir)` | Directory containing the `main` package |
| `WithIncludes(paths)` | Source files to include |
| `WithBuildFlags(flags)` | Additional `go build` flags |
| `WithBuildPath(path)` | Custom build path |
| `WithEnvironments(vars)` | Environment variables for the build |
| `WithSecrets(map)` | Build-time secrets |
| `WithSourceScripts(scripts)` | Scripts to run before build |

### Multiple artifacts

Chain multiple artifacts in a single config — they share the same context and build graph:

```go
language.NewGo("lib-core", systems).
    WithIncludes([]string{"pkg", "go.mod", "go.sum"}).
    Build(ctx)

language.NewGo("bin-server", systems).
    WithBuildDirectory("cmd/server").
    WithIncludes([]string{"cmd", "pkg", "go.mod", "go.sum"}).
    Build(ctx)

language.NewGoDevelopmentEnvironment("dev-shell", systems).
    Build(ctx)
```
