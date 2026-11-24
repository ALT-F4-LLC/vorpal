from vorpal_sdk import SYSTEMS, ConfigContext, Python, PythonDevelopmentEnvironment

ctx = ConfigContext.create()

## -> 1. Activate: `source "$(vorpal build --path 'hello-world-dev')/bin/activate"`
## -> 2. Deactivate: `deactivate`

(
    PythonDevelopmentEnvironment("hello-world-dev", SYSTEMS)
    .build(ctx)
)

## -> 1. Build: `vorpal build 'hello-world'`
## -> 2. Run: `$(vorpal build --path 'hello-world')/bin/hello-world`

(
    Python("hello-world", SYSTEMS)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

ctx.run()
