#!/usr/bin/env python3
"""Rewrite grpcio-tools proto-root-relative imports to package-absolute ones.

grpcio-tools emits cross-package imports as ``from artifact import artifact_pb2``
(relative to ``--proto_path``), which do not resolve once the generated tree lives
under the ``vorpal_sdk.api`` package. protoc has no option to qualify these imports
(grpc/grpc#9450), so the ``make generate`` Python arm runs this fixup over the freshly
generated ``api/`` tree. It also drops an ``__init__.py`` into each package dir so the
generated modules are importable as ``vorpal_sdk.api.<svc>.<svc>_pb2``.

Deterministic and reproducible: identical input always yields identical output, so the
committed bindings match a fresh regen (the C5 drift gate).
"""

import re
import sys
from pathlib import Path

# Top-level proto packages, matching sdk/rust/api/<svc>/<svc>.proto.
PACKAGES = ("agent", "archive", "artifact", "context", "worker")

_IMPORT_RE = re.compile(rf"^from ({'|'.join(PACKAGES)}) import ", re.MULTILINE)


def main(api_dir: str) -> int:
    api = Path(api_dir)
    if not api.is_dir():
        print(f"api dir not found: {api}", file=sys.stderr)
        return 1

    for path in sorted(api.rglob("*")):
        if path.suffix in (".py", ".pyi") and path.name != "__init__.py":
            text = path.read_text()
            fixed = _IMPORT_RE.sub(r"from vorpal_sdk.api.\1 import ", text)
            if fixed != text:
                path.write_text(fixed)

    for pkg_dir in (api, *(api / p for p in PACKAGES)):
        (pkg_dir / "__init__.py").write_text("")

    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else "src/vorpal_sdk/api"))
