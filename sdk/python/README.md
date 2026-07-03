# vorpal-sdk

Python SDK for building Vorpal artifacts.

Mirrors [`@altf4llc/vorpal-sdk`](https://www.npmjs.com/package/@altf4llc/vorpal-sdk) (TypeScript) and the Go SDK (`github.com/ALT-F4-LLC/vorpal/sdk/go`) in structure and public surface. Authors use this SDK to define build artifacts in Python via a `Vorpal.py.toml` config.

## Installation

```
pip install vorpal-sdk
```

Or with uv:

```
uv add vorpal-sdk
```

## Usage

```python
# vorpal.py
from vorpal_sdk import ConfigContext, Artifact

# (Phase 5+) Build your artifact graph and hand it to ConfigContext.
```

## Requirements

- Python 3.13 (pinned via `.python-version` + `requires-python`)
- Runtime dependencies: `grpcio`, `protobuf`

## Development

```bash
uv sync --frozen
uv run pytest
```

## License

Apache-2.0 — see [LICENSE](https://github.com/ALT-F4-LLC/vorpal/blob/main/LICENSE).
