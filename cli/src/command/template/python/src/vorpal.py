from vorpal_sdk import ConfigContext, Python

# Define build context

ctx = ConfigContext.create()

# Define supported artifact systems

systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
]

# Define application artifact

(
    Python("example", systems)
    .with_entrypoint("src/main.py")
    .with_includes(["pyproject.toml", "uv.lock", "src"])
    .build(ctx)
)

# Run context to build

ctx.run()
