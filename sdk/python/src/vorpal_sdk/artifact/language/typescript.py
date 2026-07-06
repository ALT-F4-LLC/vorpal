"""TypeScript project + development-environment artifact builders.

Mirrors ``sdk/typescript/src/artifact/language/typescript.js``. The build script
must match the Rust and Go SDKs character-for-character for the same inputs.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import (
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    get_env_key,
    secrets_to_proto,
)
from vorpal_sdk.artifact.bun import Bun
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


class TypeScript:
    """Builder for TypeScript project artifacts.

    Binary mode (``entrypoint`` set) compiles a standalone binary via
    ``bun build --compile``. Library mode (default) builds via ``tsc`` and copies
    ``package.json``, ``dist/``, and ``node_modules/`` to ``$VORPAL_OUTPUT``.
    """

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._aliases: list[str] = []
        self._artifacts: list[str] = []
        self._entrypoint: str | None = None
        self._environments: list[str] = []
        self._includes: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._source_scripts: list[str] = []
        self._systems = systems
        self._working_dir: str | None = None

    def with_aliases(self, aliases: list[str]) -> TypeScript:
        self._aliases = aliases
        return self

    def with_artifacts(self, artifacts: list[str]) -> TypeScript:
        self._artifacts = artifacts
        return self

    def with_entrypoint(self, entrypoint: str) -> TypeScript:
        self._entrypoint = entrypoint
        return self

    def with_environments(self, environments: list[str]) -> TypeScript:
        self._environments = environments
        return self

    def with_includes(self, includes: list[str]) -> TypeScript:
        self._includes = includes
        return self

    def with_secrets(self, secrets: dict[str, str]) -> TypeScript:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def with_source_scripts(self, scripts: list[str]) -> TypeScript:
        for script in scripts:
            if script not in self._source_scripts:
                self._source_scripts.append(script)
        return self

    def with_working_dir(self, directory: str) -> TypeScript:
        self._working_dir = directory
        return self

    def build(self, context: ConfigContext) -> str:
        bun_digest = Bun().build(context)
        bun_bin = f"{get_env_key(bun_digest)}/bin"

        source_path = "."
        source_builder = ArtifactSource(self._name, source_path)
        if len(self._includes) > 0:
            source_builder.with_includes(self._includes)
        source = source_builder.build()

        step_source_dir = f"{source_path}/source/{source.name}"
        if self._working_dir is not None:
            step_source_dir = f"{step_source_dir}/{self._working_dir}"

        if self._entrypoint is not None:
            step_build_command = (
                f"mkdir -p $VORPAL_OUTPUT/bin\n\n"
                f"{bun_bin}/bun build --compile {self._entrypoint} "
                f"--outfile {self._name}\n\n"
                f"cp {self._name} $VORPAL_OUTPUT/bin/{self._name}"
            )
        else:
            step_build_command = (
                f"mkdir -p $VORPAL_OUTPUT\n\n"
                f"{bun_bin}/bun x tsc --project tsconfig.json --outDir dist\n\n"
                f"cp package.json $VORPAL_OUTPUT/\n"
                f"cp -r dist $VORPAL_OUTPUT/\n"
                f"cp -r node_modules $VORPAL_OUTPUT/"
            )

        source_scripts = "\n".join(self._source_scripts)
        step_script = (
            f"pushd {step_source_dir}\n\n"
            f"{source_scripts}\n\n"
            f"{bun_bin}/bun install --frozen-lockfile\n\n"
            f"{step_build_command}"
        )

        step_environments = [f"PATH={bun_bin}", *self._environments]
        step_artifacts = [bun_digest, *self._artifacts]

        step = shell(
            context,
            step_artifacts,
            step_environments,
            step_script,
            secrets_to_proto(self._secrets),
        )

        return (
            Artifact(self._name, [step], self._systems)
            .with_aliases(self._aliases)
            .with_sources([source])
            .build(context)
        )


class TypeScriptDevelopmentEnvironment:
    """Builder for TypeScript development-environment artifacts (Bun only)."""

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems = systems

    def with_artifacts(
        self, artifacts: list[str]
    ) -> TypeScriptDevelopmentEnvironment:
        self._artifacts.extend(artifacts)
        return self

    def with_environments(
        self, environments: list[str]
    ) -> TypeScriptDevelopmentEnvironment:
        self._environments.extend(environments)
        return self

    def with_secrets(
        self, secrets: dict[str, str]
    ) -> TypeScriptDevelopmentEnvironment:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: ConfigContext) -> str:
        bun = Bun().build(context)

        artifacts = [bun, *self._artifacts]

        devenv = (
            DevelopmentEnvironment(self._name, self._systems)
            .with_artifacts(artifacts)
            .with_environments(self._environments)
        )

        if len(self._secrets) > 0:
            devenv = devenv.with_secrets(self._secrets)

        return devenv.build(context)
