"""System-string <-> ``ArtifactSystem`` mapping and platform detection.

Mirrors ``sdk/typescript/src/system.ts`` and ``sdk/go/pkg/config/system.go``.
The canonical system strings are ``{arch}-{os}`` (e.g. ``aarch64-darwin``).
"""

from __future__ import annotations

import platform

from vorpal_sdk.api.artifact import artifact_pb2

_STRING_TO_SYSTEM: dict[str, artifact_pb2.ArtifactSystem] = {
    "aarch64-darwin": artifact_pb2.AARCH64_DARWIN,
    "aarch64-linux": artifact_pb2.AARCH64_LINUX,
    "x86_64-darwin": artifact_pb2.X8664_DARWIN,
    "x86_64-linux": artifact_pb2.X8664_LINUX,
}

_SYSTEM_TO_STRING: dict[artifact_pb2.ArtifactSystem, str] = {
    v: k for k, v in _STRING_TO_SYSTEM.items()
}


def get_system_default_str() -> str:
    """Return the ``{arch}-{os}`` string for the current platform."""
    machine = platform.machine()
    if machine in ("arm64", "aarch64"):
        arch = "aarch64"
    elif machine in ("x86_64", "amd64", "x64"):
        arch = "x86_64"
    else:
        arch = machine

    system = platform.system()
    if system == "Darwin":
        os_name = "darwin"
    elif system == "Linux":
        os_name = "linux"
    else:
        os_name = system.lower()

    return f"{arch}-{os_name}"


def get_system(system: str) -> artifact_pb2.ArtifactSystem:
    """Map a system string to its ``ArtifactSystem`` enum value."""
    try:
        return _STRING_TO_SYSTEM[system]
    except KeyError:
        raise ValueError(f"unsupported system: {system}") from None


def get_system_default() -> artifact_pb2.ArtifactSystem:
    """Return the ``ArtifactSystem`` enum value for the current platform."""
    return get_system(get_system_default_str())


def get_system_str(system: artifact_pb2.ArtifactSystem) -> str:
    """Map an ``ArtifactSystem`` enum value to its system string."""
    return _SYSTEM_TO_STRING.get(system, "unknown")
