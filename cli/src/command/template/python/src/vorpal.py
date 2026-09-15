from vorpal_sdk import SYSTEMS, ConfigContext, Python, PythonDevelopmentEnvironment

ctx = ConfigContext.create()

## -> 1. Activate: `source "$(vorpal build --path 'example-dev')/bin/activate"`
## -> 2. Deactivate: `deactivate`

(
    PythonDevelopmentEnvironment("example-dev", SYSTEMS)
    .build(ctx)
)

## -> 1. Build: `vorpal build 'example'`
## -> 2. Run: `$(vorpal build --path 'example')/bin/example`

(
    Python("example", SYSTEMS)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

ctx.run()
