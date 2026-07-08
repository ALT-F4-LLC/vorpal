---
title: Python SDK
description: Build artifacts and environments with the Vorpal Python SDK.
---

The Python SDK lets you define Vorpal build configurations as Python programs. Your config runs as a Python program that communicates with the Vorpal daemon over gRPC.

## Installation

Add the SDK to your Python project:

```bash
pip install vorpal-sdk
```

Or, if you use uv:

```bash
uv add vorpal-sdk
```

## Project setup

Create a `Vorpal.toml` manifest in your project root:

```toml title="Vorpal.toml"
language = "python"

[source]
includes = [
    "pyproject.toml",
    "uv.lock",
    "src",
]

[source.python]
directory = "."
entrypoint = "src/vorpal.py"
```

The `language` field tells Vorpal to use the Python SDK. `includes` lists only the files Vorpal needs to track; keeping this minimal maximizes caching between artifacts. `[source.python]` sets the directory and entrypoint for the build config.

Define your project dependencies in `pyproject.toml`:

```toml title="pyproject.toml"
[project]
name = "example"
version = "0.1.0"
requires-python = ">=3.13,<3.14"
dependencies = [
    "vorpal-sdk>=0.3.0",
]

[tool.uv]
package = false
```

Then create a build configuration in `src/vorpal.py`:

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

# Define your artifacts here

ctx.run()
```

Every Vorpal config starts by creating a context and defining target systems as canonical system strings. The context manages the connection to the Vorpal daemon and tracks all artifacts.

## Defining artifacts

Artifacts are the core building blocks in Vorpal. Each artifact defines what to build, which platforms to target, what files to include, and more.

### Define an artifact

Use the `Python` builder to package a Python project into a cross-platform artifact:

:::note
`Python` is a language-specific abstraction over the generic [Artifact](/concepts/artifacts/) type.
:::

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Python

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

(
    Python("example", systems)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

ctx.run()
```

The `Python` builder:
- **`with_entrypoint`** - Sets the Python file used for the executable launcher
- **`with_includes`** - Lists files and directories to include in the build source

The `Python` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_aliases(aliases)` | Alternative names for the artifact |
| `with_artifacts(artifacts)` | Artifact dependencies available during build |
| `with_entrypoint(entrypoint)` | Python file used for the executable launcher |
| `with_environments(environments)` | Environment variables for the build |
| `with_includes(includes)` | Source files to include |
| `with_secrets(secrets)` | Build-time secrets |
| `with_source_scripts(scripts)` | Scripts to run before the build |
| `with_working_dir(directory)` | Working directory inside the source tree |

See [Artifacts](/concepts/artifacts/) to learn more.

### Define artifact dependencies

Build artifacts like `protoc` and pass them as dependencies to your language artifact:

:::note
`Protoc` is a built-in artifact provided by the Vorpal SDK. See [Built-in artifacts](/concepts/artifacts/#built-in-artifacts) for the full list.
:::

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Protoc, Python

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

protoc = Protoc().build(ctx)

(
    Python("example", systems)
    .with_artifacts([protoc])
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

ctx.run()
```

The dependent artifact's output is available at `$VORPAL_ARTIFACT_<digest>` during execution. Use `get_env_key` to resolve the path.

See [Artifacts](/concepts/artifacts/) to learn more.

### Define development environments

Create a portable development shell with pinned tools, environment variables, and more:

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Protoc, PythonDevelopmentEnvironment

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

protoc = Protoc().build(ctx)

(
    PythonDevelopmentEnvironment("my-project-shell", systems)
    .with_artifacts([protoc])
    .with_environments(["PYTHONWARNINGS=default"])
    .build(ctx)
)

ctx.run()
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

The `PythonDevelopmentEnvironment` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available in the shell |
| `with_environments(environments)` | Environment variables set in the shell |
| `with_secrets(secrets)` | Secrets available in the shell |

See [Environments](/concepts/environments/) to learn more.

### Define jobs

Jobs run scripts that never cache by default - ideal for CI tasks, tests, and automation.

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Job, Python, get_env_key

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

example = (
    Python("example", systems)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

script = f"{get_env_key(example)}/bin/example --version"

(
    Job("my-job", script, systems)
    .with_artifacts([example])
    .build(ctx)
)

ctx.run()
```

The `Job` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_secrets(secrets)` | Secrets available during execution |

See [Jobs](/concepts/jobs/) to learn more.

### Define processes

Processes wrap long-running binaries with start, stop, and logs lifecycle scripts.

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Process, Python, get_env_key

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

example = (
    Python("example", systems)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

(
    Process(
        "my-server",
        f"{get_env_key(example)}/bin/example",
        systems,
    )
    .with_arguments(["--port", "8080"])
    .with_artifacts([example])
    .build(ctx)
)

ctx.run()
```

The `Process` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_arguments(arguments)` | Command-line arguments for the process |
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_secrets(secrets)` | Secrets available during execution |

See [Processes](/concepts/processes/) to learn more.

### Define user environments

Install tools into your user-wide environment with symlinks:

```python title="src/vorpal.py"
from vorpal_sdk import ConfigContext, Python, UserEnvironment, get_env_key

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

example = (
    Python("example", systems)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

(
    UserEnvironment("my-home", systems)
    .with_artifacts([example])
    .with_symlinks([
        (f"{get_env_key(example)}/bin/example", "$HOME/.vorpal/bin/example"),
    ])
    .build(ctx)
)

ctx.run()
```

Activate with `$HOME/.vorpal/bin/vorpal-activate`, then `source $HOME/.vorpal/bin/vorpal-activate-shell`.

The `UserEnvironment` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_artifacts(artifacts)` | Artifact dependencies available in the environment |
| `with_environments(environments)` | Environment variables set in the environment |
| `with_symlinks(symlinks)` | Symlinks to create from artifact outputs to local paths |

See [Environments](/concepts/environments/) to learn more.

## Custom executors

Replace the default Bash executor with Docker or any custom binary:

```python title="src/vorpal.py"
from vorpal_sdk import Artifact, ArtifactStep, ConfigContext

ctx = ConfigContext.create()

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

step = (
    ArtifactStep("docker")
    .with_arguments([
        "run", "--rm", "-v", "$VORPAL_OUTPUT:/out",
        "alpine", "sh", "-lc",
        "echo hi > /out/hi.txt",
    ])
    .build()
)

Artifact("example-docker", [step], systems).build(ctx)

ctx.run()
```

The `ArtifactStep` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_arguments(arguments)` | Arguments passed to the entrypoint |
| `with_artifacts(artifacts)` | Artifact dependencies available during execution |
| `with_environments(environments)` | Environment variables for the step |
| `with_script(script)` | Script to execute in the step |
| `with_secrets(secrets)` | Secrets available during execution |

The `Artifact` builder supports additional configuration:

| Method | Description |
|--------|-------------|
| `with_aliases(aliases)` | Alternative names for the artifact |
| `with_sources(sources)` | Source files to include in the artifact |

See [Artifacts](/concepts/artifacts/) to learn more.

## Building

Run your config with the Vorpal CLI:

```bash
vorpal build my-app
```

For the template above, use the artifact name from the config:

```bash
vorpal build example
```

First builds download toolchains and dependencies. Subsequent builds with the same inputs resolve instantly from the content-addressed cache.
