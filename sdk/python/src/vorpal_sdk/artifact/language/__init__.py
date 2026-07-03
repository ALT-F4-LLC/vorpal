"""Language-specific artifact builders (Go, Python, Rust, TypeScript).

Mirrors ``sdk/typescript/src/artifact/language/*``. Each module exposes a
project builder plus its matching ``*DevelopmentEnvironment``. Re-exports for
the public API live in the top-level ``vorpal_sdk`` package, not here, to keep
this package free of the tool<->language import cycle.
"""
