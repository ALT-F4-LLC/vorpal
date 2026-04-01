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

Create a `Vorpal.toml` manifest in your project root:

:::note
This example follows the <a href="https://github.com/golang-standards/project-layout" target="_blank">Standard Go Project Layout</a>.
:::

```toml title="Vorpal.toml"
language = "go"

[source]
includes = ["cmd/vorpal", "go.mod", "go.sum"]

[source.go]
directory = "cmd/vorpal"
```

The `language` field tells Vorpal to use the Go SDK. `includes` lists only the files Vorpal needs to track — keeping this minimal maximizes caching between artifacts. `[source.go]` sets the directory containing your build config's `main` package.

Then create a build configuration in `cmd/vorpal/main.go`:

```go title="cmd/vorpal/main.go"
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    // Define your artifacts here

    ctx.Run()
}
```

Every Vorpal config starts by creating a context and defining target systems. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

Artifacts are the core building blocks in Vorpal. Each artifact defines what to build, which platforms to target, what files to include, and more.

### Define an artifact

Use the `Go` builder from `language` package to compile a Go project into a cross-platform artifact:

:::note
`Go` is a language-specific abstraction over the generic [Artifact](/concepts/artifacts/) type.
:::

```go title="cmd/vorpal/main.go" {5,7,20-26}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    _, err := language.NewGo("my-app", systems).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd/my-app", "go.mod", "go.sum"}).
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

The `Go` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithAliases(aliases)` | Alternative names for the artifact |
| `WithArtifacts(artifacts)` | Artifact dependencies available during build |
| `WithBuildDirectory(dir)` | Directory containing the `main` package |
| `WithBuildFlags(flags)` | Additional `go build` flags |
| `WithBuildPath(path)` | Custom build path |
| `WithEnvironments(vars)` | Environment variables for the build |
| `WithIncludes(paths)` | Source files to include |
| `WithSecrets(map)` | Build-time secrets |
| `WithSource(source)` | Custom artifact source |
| `WithSourceScript(script)` | Script to run before build |

See [Artifacts](/concepts/artifacts/) to learn more.

### Define artifact dependencies

Build artifacts like `protoc` and pass them as dependencies to your language artifact:

:::note
`Protoc` is an artifact builder provided by the Vorpal SDK. See [Built-in artifacts](/concepts/artifacts/#built-in-artifacts) for the full list.
:::

```go title="cmd/vorpal/main.go" {5,21-24,27}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    protoc, err := artifact.Protoc(ctx)
    if err != nil {
        log.Fatalf("error building protoc: %v", err)
    }

    _, err = language.NewGo("my-app", systems).
        WithArtifacts([]*string{protoc}).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd/my-app", "go.mod", "go.sum"}).
        Build(ctx)
    if err != nil {
        log.Fatalf("error building: %v", err)
    }

    ctx.Run()
}
```

The dependent artifact's output is available at `$VORPAL_ARTIFACT_<digest>` during execution. Use `GetEnvKey` to resolve the path.

See [Artifacts](/concepts/artifacts/) to learn more.

### Define development environments

Create a portable development shell with pinned tools, environment variables, and more:

```go title="cmd/vorpal/main.go" {6, 26-29}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    protoc, err := artifact.Protoc(ctx)
    if err != nil {
        log.Fatalf("error building protoc: %v", err)
    }

    language.NewGoDevelopmentEnvironment("my-project-shell", systems).
        WithArtifacts([]*string{protoc}).
        WithEnvironments([]string{"CGO_ENABLED=0"}).
        Build(ctx)

    ctx.Run()
}
```

Activate the environment:

```bash title="Terminal"
source $(vorpal build --path my-project-shell)/bin/activate
```

Verify that dependencies are coming from the Vorpal store:

```bash title="Terminal"
$ which protoc
/var/lib/vorpal/store/artifact/output/library/512b7dd.../bin/protoc
```

To exit, run `deactivate` or close the shell.

The development environment builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithArtifacts(artifacts)` | Artifact dependencies available in the shell |
| `WithEnvironments(vars)` | Environment variables set in the shell |
| `WithoutProtoc()` | Exclude the default Protoc artifact |
| `WithSecrets(map)` | Secrets available in the shell |

See [Environments](/concepts/environments/) to learn more.

### Define jobs

Jobs run scripts that never cache by default — ideal for CI tasks, tests, and automation.

```go title="cmd/vorpal/main.go" {6,36-38,40-42}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "fmt"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    protoc, err := artifact.Protoc(ctx)
    if err != nil {
        log.Fatalf("error building protoc: %v", err)
    }

    myApp, err := language.NewGo("my-app", systems).
        WithArtifacts([]*string{protoc}).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd/my-app", "go.mod", "go.sum"}).
        Build(ctx)
    if err != nil {
        log.Fatalf("error building: %v", err)
    }

    script := fmt.Sprintf(`
        %s/bin/my-app --version
    `, artifact.GetEnvKey(*myApp))

    artifact.NewJob("my-job", script, systems).
        WithArtifacts([]*string{myApp}).
        Build(ctx)

    ctx.Run()
}
```

The `Job` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithArtifacts(artifacts)` | Artifact dependencies available during execution |
| `WithSecrets(map)` | Secrets available during execution |

See [Jobs](/concepts/jobs/) to learn more.

### Define processes

Processes wrap long-running binaries with start, stop, and logs lifecycle scripts.

```go title="cmd/vorpal/main.go" {6,36-43}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "fmt"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    protoc, err := artifact.Protoc(ctx)
    if err != nil {
        log.Fatalf("error building protoc: %v", err)
    }

    myApp, err := language.NewGo("my-app", systems).
        WithArtifacts([]*string{protoc}).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd/my-app", "go.mod", "go.sum"}).
        Build(ctx)
    if err != nil {
        log.Fatalf("error building: %v", err)
    }

    artifact.NewProcess(
        "my-server",
        fmt.Sprintf("%s/bin/my-server", artifact.GetEnvKey(*myApp)),
        systems,
    ).
        WithArguments([]string{"--port", "8080"}).
        WithArtifacts([]*string{myApp}).
        Build(ctx)

    ctx.Run()
}
```

The `Process` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithArguments(args)` | Command-line arguments for the process |
| `WithArtifacts(artifacts)` | Artifact dependencies available during execution |
| `WithSecrets(map)` | Secrets available during execution |

See [Processes](/concepts/processes/) to learn more.

### Define user environments

Install tools into your user-wide environment with symlinks:

```go title="cmd/vorpal/main.go" {5,30-33}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact/language"
    "fmt"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
    "log"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    myApp, err := language.NewGo("my-app", systems).
        WithBuildDirectory("cmd/my-app").
        WithIncludes([]string{"cmd/my-app", "go.mod", "go.sum"}).
        Build(ctx)
    if err != nil {
        log.Fatalf("error building: %v", err)
    }

    artifact.NewUserEnvironment("my-home", systems).
        WithArtifacts([]*string{myApp}).
        WithSymlinks(map[string]string{fmt.Sprintf("%s/bin/my-app", artifact.GetEnvKey(*myApp)): "$HOME/.vorpal/bin/my-app"}).
        Build(ctx)

    ctx.Run()
}
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

The `UserEnvironment` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithArtifacts(artifacts)` | Artifact dependencies available in the environment |
| `WithEnvironments(vars)` | Environment variables set in the environment |
| `WithSymlinks(links)` | Symlinks to create from artifact outputs to local paths |

See [Environments](/concepts/environments/) to learn more.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```go title="cmd/vorpal/main.go" {5,19-25,27-28}
package main

import (
    api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
    "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func main() {
    ctx := config.GetContext()

    systems := []api.ArtifactSystem{
        api.ArtifactSystem_AARCH64_DARWIN,
        api.ArtifactSystem_AARCH64_LINUX,
        api.ArtifactSystem_X8664_DARWIN,
        api.ArtifactSystem_X8664_LINUX,
    }

    step := artifact.NewArtifactStep("docker").
        WithArguments([]string{
            "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
            "alpine", "sh", "-lc",
            "echo hi > /out/hi.txt",
        }).
        Build()

    artifact.NewArtifact("example-docker",
        []*api.ArtifactStep{step}, systems).Build(ctx)

    ctx.Run()
}
```

The `ArtifactStep` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithArguments(args)` | Arguments passed to the entrypoint |
| `WithArtifacts(artifacts)` | Artifact dependencies available during execution |
| `WithEnvironments(vars)` | Environment variables for the step |
| `WithScript(script)` | Script to execute in the step |
| `WithSecrets(secrets)` | Secrets available during execution |

The `Artifact` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `WithAliases(aliases)` | Alternative names for the artifact |
| `WithSources(sources)` | Source files to include in the artifact |

See [Artifacts](/concepts/artifacts/) to learn more.

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.
