"""Step builders (``bash``/``bwrap``/``shell``/``docker``).

Mirrors ``sdk/typescript/src/artifact/step.ts`` (the canonical reference) and
``sdk/go/pkg/artifact/step.go``. Sync model — no async, unlike the TS SDK.

These are pure functions of their inputs except ``shell``, which dispatches on
the target system via ``BuildContext.get_system``. The Linux branch needs the
``linux-vorpal`` rootfs builder (a later phase); until it lands, ``shell``
raises on Linux targets. The Darwin branch (``bash``) is fully implemented.
"""

from __future__ import annotations

from typing import Protocol, runtime_checkable

from vorpal_sdk.api.artifact import artifact_pb2

_DEFAULT_PATH = "/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"


@runtime_checkable
class BuildContext(Protocol):
    """The slice of ``ConfigContext`` (Phase 5) the builders depend on.

    Defining it as a Protocol lets Phase-4 builders type-check against the
    not-yet-implemented context and lets tests supply a lightweight stub.
    """

    def get_system(self) -> artifact_pb2.ArtifactSystem: ...

    def add_artifact(self, artifact: artifact_pb2.Artifact) -> str: ...

    def get_variable(self, name: str) -> str | None: ...

    def get_artifact_namespace(self) -> str: ...


def _dedupe_secrets(
    secrets: list[artifact_pb2.ArtifactStepSecret],
) -> list[artifact_pb2.ArtifactStepSecret]:
    seen: set[str] = set()
    unique: list[artifact_pb2.ArtifactStepSecret] = []
    for secret in secrets:
        if secret.name not in seen:
            seen.add(secret.name)
            unique.append(secret)
    return unique


def bash(
    artifacts: list[str],
    environments: list[str],
    secrets: list[artifact_pb2.ArtifactStepSecret],
    script: str,
) -> artifact_pb2.ArtifactStep:
    """Build a bash step (Darwin). Matches Rust/Go/TS ``bash``."""
    # Deferred import breaks the artifact<->step cycle (artifact imports shell).
    from vorpal_sdk.artifact import get_env_key

    step_environments = [e for e in environments if not e.startswith("PATH=")]

    step_path_bins = ":".join(f"{get_env_key(a)}/bin" for a in artifacts)
    step_path = f"{step_path_bins}:{_DEFAULT_PATH}"

    for environment in environments:
        if environment.startswith("PATH="):
            path_value = environment[len("PATH=") :]
            if path_value:
                step_path = f"{path_value}:{step_path}"

    step_environments.append("HOME=$VORPAL_WORKSPACE")
    step_environments.append(f"PATH={step_path}")

    step_script = f"#!/bin/bash\nset -euo pipefail\n\n{script}\n"

    return artifact_pb2.ArtifactStep(
        entrypoint="bash",
        script=step_script,
        secrets=_dedupe_secrets(secrets),
        arguments=[],
        artifacts=artifacts,
        environments=step_environments,
    )


def bwrap(
    arguments: list[str],
    artifacts: list[str],
    environments: list[str],
    rootfs: str | None,
    secrets: list[artifact_pb2.ArtifactStepSecret],
    script: str,
) -> artifact_pb2.ArtifactStep:
    """Build a bwrap step (Linux). Matches Rust/Go/TS ``bwrap``.

    CRITICAL: argument-list ordering must be identical to Rust/Go/TS.
    """
    from vorpal_sdk.artifact import get_env_key

    step_arguments: list[str] = [
        "--unshare-all",
        "--share-net",
        "--clearenv",
        "--chdir",
        "$VORPAL_WORKSPACE",
        "--gid",
        "1000",
        "--uid",
        "1000",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--bind",
        "$VORPAL_OUTPUT",
        "$VORPAL_OUTPUT",
        "--bind",
        "$VORPAL_WORKSPACE",
        "$VORPAL_WORKSPACE",
        "--setenv",
        "VORPAL_OUTPUT",
        "$VORPAL_OUTPUT",
        "--setenv",
        "VORPAL_WORKSPACE",
        "$VORPAL_WORKSPACE",
        "--setenv",
        "HOME",
        "$VORPAL_WORKSPACE",
    ]

    step_artifacts: list[str] = []

    if rootfs is not None:
        rootfs_env = get_env_key(rootfs)
        step_arguments.extend(
            [
                "--ro-bind",
                f"{rootfs_env}/bin",
                "/bin",
                "--ro-bind",
                f"{rootfs_env}/etc",
                "/etc",
                "--ro-bind",
                f"{rootfs_env}/lib",
                "/lib",
                "--ro-bind-try",
                f"{rootfs_env}/lib64",
                "/lib64",
                "--ro-bind",
                f"{rootfs_env}/sbin",
                "/sbin",
                "--ro-bind",
                f"{rootfs_env}/usr",
                "/usr",
            ]
        )
        step_artifacts.append(rootfs)

    step_artifacts.extend(artifacts)

    for artifact in step_artifacts:
        env_key = get_env_key(artifact)
        step_arguments.append("--ro-bind")
        step_arguments.append(env_key)
        step_arguments.append(env_key)
        step_arguments.append("--setenv")
        step_arguments.append(env_key.replace("$", ""))
        step_arguments.append(env_key)

    step_path_bins = ":".join(f"{get_env_key(a)}/bin" for a in step_artifacts)
    step_path = f"{step_path_bins}:{_DEFAULT_PATH}"

    for environment in environments:
        if environment.startswith("PATH="):
            path_value = environment[len("PATH=") :]
            if path_value:
                step_path = f"{path_value}:{step_path}"

    step_arguments.append("--setenv")
    step_arguments.append("PATH")
    step_arguments.append(step_path)

    for environment in environments:
        eq_idx = environment.find("=")
        key = environment[:eq_idx] if eq_idx != -1 else environment
        value = environment[eq_idx + 1 :] if eq_idx != -1 else ""
        if key.startswith("PATH"):
            continue
        step_arguments.append("--setenv")
        step_arguments.append(key)
        step_arguments.append(value)

    step_arguments.extend(arguments)

    step_script = f"#!/bin/bash\nset -euo pipefail\n\n{script}\n"

    return artifact_pb2.ArtifactStep(
        entrypoint="bwrap",
        script=step_script,
        secrets=_dedupe_secrets(secrets),
        arguments=step_arguments,
        artifacts=step_artifacts,
        environments=[f"PATH={_DEFAULT_PATH}"],
    )


def shell(
    context: BuildContext,
    artifacts: list[str],
    environments: list[str],
    script: str,
    secrets: list[artifact_pb2.ArtifactStepSecret],
) -> artifact_pb2.ArtifactStep:
    """Dispatch to ``bash`` on Darwin, ``bwrap`` on Linux.

    Matches Rust/Go/TS ``shell``. The Linux branch requires the
    ``linux-vorpal`` rootfs builder, deferred to a later phase.
    """
    step_system = context.get_system()

    if step_system in (
        artifact_pb2.AARCH64_DARWIN,
        artifact_pb2.X8664_DARWIN,
    ):
        return bash(artifacts, environments, secrets, script)

    if step_system in (
        artifact_pb2.AARCH64_LINUX,
        artifact_pb2.X8664_LINUX,
    ):
        raise NotImplementedError(
            "shell() on Linux targets requires the linux-vorpal rootfs builder, "
            "which lands in a later phase"
        )

    raise ValueError(f"unsupported system: {step_system}")


def docker(
    arguments: list[str],
    artifacts: list[str],
) -> artifact_pb2.ArtifactStep:
    """Build a docker step. Matches Rust/Go/TS ``docker``."""
    return artifact_pb2.ArtifactStep(
        entrypoint="docker",
        secrets=[],
        arguments=arguments,
        artifacts=artifacts,
        environments=[f"PATH={_DEFAULT_PATH}"],
    )
