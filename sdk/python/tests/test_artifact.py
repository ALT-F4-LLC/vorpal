"""Message-shape unit tests for the core artifact builders.

Each builder is a pure function of its inputs; these assert the produced proto
message shape (entrypoint, script text, secret order, dedup, optional-field
presence) in isolation, with a lightweight stub context. Digest parity against
the Go/TS builders is covered separately in ``test_builder_parity.py``.

Runnable two ways:
  * ``pytest``, or
  * ``python tests/test_artifact.py`` (dependency-free runner).
"""

from __future__ import annotations

import sys
from pathlib import Path

_SRC = Path(__file__).resolve().parents[1] / "src"
if str(_SRC) not in sys.path:
    sys.path.insert(0, str(_SRC))

from vorpal_sdk.api.artifact import artifact_pb2  # noqa: E402
from vorpal_sdk.artifact import (  # noqa: E402
    Argument,
    Artifact,
    ArtifactSource,
    ArtifactStep,
    DevelopmentEnvironment,
    Job,
    OciImage,
    Process,
    UserEnvironment,
    get_env_key,
    secrets_to_proto,
)
from vorpal_sdk.step import bash, docker, shell  # noqa: E402

_DARWIN = artifact_pb2.AARCH64_DARWIN


class _StubContext:
    """Captures the artifact passed to ``add_artifact`` for shape assertions."""

    def __init__(self, variables: dict[str, str] | None = None) -> None:
        self.variables = variables or {}
        self.last: artifact_pb2.Artifact | None = None

    def get_system(self) -> artifact_pb2.ArtifactSystem:
        return _DARWIN

    def get_variable(self, name: str) -> str | None:
        return self.variables.get(name)

    def get_artifact_namespace(self) -> str:
        return "altf4llc"

    def add_artifact(self, artifact: artifact_pb2.Artifact) -> str:
        self.last = artifact
        return "stub-digest"


def _only_step(ctx: _StubContext) -> artifact_pb2.ArtifactStep:
    assert ctx.last is not None
    assert len(ctx.last.steps) == 1
    return ctx.last.steps[0]


# --- helpers ---------------------------------------------------------------


def test_get_env_key_format() -> None:
    assert get_env_key("abc123") == "$VORPAL_ARTIFACT_abc123"


def test_secrets_to_proto_sorts_by_name() -> None:
    out = secrets_to_proto({"zebra": "1", "alpha": "2", "mid": "3"})
    assert [s.name for s in out] == ["alpha", "mid", "zebra"]
    assert [s.value for s in out] == ["2", "3", "1"]


# --- ArtifactSource --------------------------------------------------------


def test_artifact_source_absent_digest_is_unset() -> None:
    src = ArtifactSource("n", "/p").build()
    assert src.HasField("digest") is False
    assert src.name == "n"
    assert src.path == "/p"
    assert list(src.excludes) == []
    assert list(src.includes) == []


def test_artifact_source_present_digest_and_globs() -> None:
    src = (
        ArtifactSource("n", "/p")
        .with_digest("d")
        .with_excludes(["*.log"])
        .with_includes(["src/**"])
        .build()
    )
    assert src.HasField("digest") is True
    assert src.digest == "d"
    assert list(src.excludes) == ["*.log"]
    assert list(src.includes) == ["src/**"]


# --- ArtifactStep ----------------------------------------------------------


def test_artifact_step_absent_script_is_unset() -> None:
    step = ArtifactStep("bash").build()
    assert step.entrypoint == "bash"
    assert step.HasField("script") is False


def test_artifact_step_dedupes_secrets_by_name() -> None:
    dup = [
        artifact_pb2.ArtifactStepSecret(name="A", value="1"),
        artifact_pb2.ArtifactStepSecret(name="A", value="2"),
        artifact_pb2.ArtifactStepSecret(name="B", value="3"),
    ]
    step = ArtifactStep("bash").with_secrets(dup).with_script("x").build()
    assert [s.name for s in step.secrets] == ["A", "B"]
    assert step.secrets[0].value == "1"  # first wins
    assert step.script == "x"


# --- Artifact --------------------------------------------------------------


def test_artifact_dedupes_sources_and_aliases() -> None:
    ctx = _StubContext()
    step = ArtifactStep("bash").with_script("x").build()
    s1 = ArtifactSource("dup", "/a").build()
    s1b = ArtifactSource("dup", "/b").build()
    s2 = ArtifactSource("other", "/c").build()
    digest = (
        Artifact("art", [step], [_DARWIN])
        .with_sources([s1, s2, s1b])
        .with_aliases(["x", "x", "y"])
        .build(ctx)
    )
    assert digest == "stub-digest"
    assert ctx.last is not None
    assert ctx.last.target == _DARWIN
    assert [s.name for s in ctx.last.sources] == ["dup", "other"]
    assert list(ctx.last.aliases) == ["x", "y"]


# --- bash / docker / shell -------------------------------------------------


def test_bash_env_ordering_and_path_filter() -> None:
    step = bash(["d1"], ["FOO=bar", "PATH=/x"], [], "echo hi")
    assert step.entrypoint == "bash"
    assert step.script == "#!/bin/bash\nset -euo pipefail\n\necho hi\n"
    # Input PATH= is filtered from environments; HOME then PATH appended last.
    assert list(step.environments) == [
        "FOO=bar",
        "HOME=$VORPAL_WORKSPACE",
        "PATH=/x:$VORPAL_ARTIFACT_d1/bin:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
    ]


def test_docker_step_has_no_script() -> None:
    step = docker(["run", "x"], ["d1"])
    assert step.entrypoint == "docker"
    assert step.HasField("script") is False
    assert list(step.arguments) == ["run", "x"]
    assert list(step.environments) == [
        "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"
    ]


def test_shell_linux_target_not_implemented() -> None:
    class LinuxCtx(_StubContext):
        def get_system(self) -> artifact_pb2.ArtifactSystem:
            return artifact_pb2.AARCH64_LINUX

    raised = False
    try:
        shell(LinuxCtx(), [], [], "x", [])
    except NotImplementedError:
        raised = True
    assert raised


# --- Job / Process / DevelopmentEnvironment / UserEnvironment --------------


def test_job_produces_single_bash_step_with_sorted_secrets() -> None:
    ctx = _StubContext()
    Job("j", "echo hi", [_DARWIN]).with_secrets(
        {"B": "2", "A": "1"}
    ).build(ctx)
    step = _only_step(ctx)
    assert step.entrypoint == "bash"
    assert step.script == "#!/bin/bash\nset -euo pipefail\n\necho hi\n"
    assert [s.name for s in step.secrets] == ["A", "B"]


def test_process_script_contains_helper_commands() -> None:
    ctx = _StubContext()
    Process("proc", "/bin/server", [_DARWIN]).with_arguments(
        ["--port", "8080"]
    ).build(ctx)
    step = _only_step(ctx)
    assert step.entrypoint == "bash"
    assert step.script is not None
    for marker in ("proc-logs", "proc-stop", "proc-start"):
        assert marker in step.script
    assert "Process: /bin/server --port 8080" in step.script


def test_development_environment_script_has_activate_block() -> None:
    ctx = _StubContext()
    DevelopmentEnvironment("dev", [_DARWIN]).with_environments(
        ["FOO=bar"]
    ).build(ctx)
    step = _only_step(ctx)
    assert step.script is not None
    assert 'export PS1="(dev) $PS1"' in step.script
    assert 'export VORPAL_SHELL_BACKUP_FOO="$FOO"' in step.script
    assert "deactivate(){" in step.script


def test_user_environment_sorts_symlinks_by_source() -> None:
    ctx = _StubContext()
    UserEnvironment("user", [_DARWIN]).with_symlinks(
        [("/src/b", "/dst/b"), ("/src/a", "/dst/a")]
    ).build(ctx)
    step = _only_step(ctx)
    assert step.script is not None
    # Sorted by source: /src/a activate line precedes /src/b.
    a_idx = step.script.index("ln -s /src/a /dst/a")
    b_idx = step.script.index("ln -s /src/b /dst/b")
    assert a_idx < b_idx


# --- OciImage --------------------------------------------------------------


def test_oci_image_rejects_uppercase_name() -> None:
    raised = False
    try:
        OciImage("MyImage", "rootfs").with_crane("c").with_rsync("r").build(
            _StubContext()
        )
    except ValueError:
        raised = True
    assert raised


def test_oci_image_rejects_invalid_char() -> None:
    raised = False
    try:
        OciImage("my image", "rootfs").with_crane("c").with_rsync("r").build(
            _StubContext()
        )
    except ValueError:
        raised = True
    assert raised


def test_oci_image_requires_crane_and_rsync() -> None:
    raised = False
    try:
        OciImage("img", "rootfs").build(_StubContext())
    except NotImplementedError:
        raised = True
    assert raised


def test_oci_image_builds_linux_artifact_with_aliases() -> None:
    ctx = _StubContext()
    OciImage("img", "rootfs").with_crane("crane-d").with_rsync(
        "rsync-d"
    ).with_aliases(["img:latest"]).build(ctx)
    assert ctx.last is not None
    assert list(ctx.last.systems) == [
        artifact_pb2.AARCH64_LINUX,
        artifact_pb2.X8664_LINUX,
    ]
    assert list(ctx.last.aliases) == ["img:latest"]
    step = ctx.last.steps[0]
    assert step.script is not None
    assert 'OCI_IMAGE_NAME="img"' in step.script


# --- Argument --------------------------------------------------------------


def test_argument_returns_variable() -> None:
    ctx = _StubContext({"KEY": "value"})
    assert Argument("KEY").build(ctx) == "value"
    assert Argument("MISSING").build(ctx) is None


def test_argument_required_missing_raises() -> None:
    raised = False
    try:
        Argument("MISSING").with_require().build(_StubContext())
    except ValueError:
        raised = True
    assert raised


def _run() -> int:
    tests = [
        v
        for k, v in sorted(globals().items())
        if k.startswith("test_") and callable(v)
    ]
    failures = 0
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except Exception as exc:  # noqa: BLE001 - runner surfaces any failure
            failures += 1
            print(f"FAIL {t.__name__}: {exc}")
    print(f"\n{len(tests) - failures}/{len(tests)} passed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(_run())
