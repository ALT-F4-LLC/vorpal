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
