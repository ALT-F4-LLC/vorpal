"""Go project + development-environment artifact builders.

Mirrors ``sdk/typescript/src/artifact/language/go.js``. The build script must be
character-for-character identical to the Rust and Go SDKs for the same inputs.
"""

from __future__ import annotations

from collections.abc import Sequence
from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import (
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    _normalize_systems_for_build,
    _raise_systems_error,
    get_env_key,
    secrets_to_proto,
)
from vorpal_sdk.artifact.git import Git
from vorpal_sdk.artifact.go import GoBin
from vorpal_sdk.artifact.goimports import Goimports
from vorpal_sdk.artifact.gopls import Gopls
from vorpal_sdk.artifact.protoc import Protoc
from vorpal_sdk.artifact.protoc_gen_go import ProtocGenGo
from vorpal_sdk.artifact.protoc_gen_go_grpc import ProtocGenGoGrpc
from vorpal_sdk.artifact.staticcheck import Staticcheck
from vorpal_sdk.step import shell
from vorpal_sdk.system import ArtifactSystemInput

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


def get_goos(system: artifact_pb2.ArtifactSystem) -> str:
    """Map an ArtifactSystem to the Go ``GOOS`` value."""
    if system in (artifact_pb2.AARCH64_DARWIN, artifact_pb2.X8664_DARWIN):
        return "darwin"
    if system in (artifact_pb2.AARCH64_LINUX, artifact_pb2.X8664_LINUX):
        return "linux"
    raise ValueError(f"unsupported 'go' system: {system}")


def get_goarch(system: artifact_pb2.ArtifactSystem) -> str:
    """Map an ArtifactSystem to the Go ``GOARCH`` value."""
    if system in (artifact_pb2.AARCH64_DARWIN, artifact_pb2.AARCH64_LINUX):
        return "arm64"
    if system in (artifact_pb2.X8664_DARWIN, artifact_pb2.X8664_LINUX):
        return "amd64"
    raise ValueError(f"unsupported 'go' system: {system}")


class Go:
    """Builder for Go project artifacts."""

    def __init__(
        self, name: str, systems: Sequence[ArtifactSystemInput]
    ) -> None:
        self._aliases: list[str] = []
        self._artifacts: list[str] = []
        self._build_directory = "."
        self._build_flags = ""
        self._build_path = "."
        self._environments: list[str] = []
        self._includes: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._source: artifact_pb2.ArtifactSource | None = None
        self._source_scripts: list[str] = []
        self._systems, self._systems_error = _normalize_systems_for_build(
            systems
        )

    def with_aliases(self, aliases: list[str]) -> Go:
        self._aliases = aliases
        return self

    def with_artifacts(self, artifacts: list[str]) -> Go:
        self._artifacts = artifacts
        return self

    def with_build_directory(self, directory: str) -> Go:
        self._build_directory = directory
        return self

    def with_build_flags(self, flags: str) -> Go:
        self._build_flags = flags
        return self

    def with_build_path(self, path: str) -> Go:
        self._build_path = path
        return self

    def with_environments(self, environments: list[str]) -> Go:
        self._environments = environments
        return self

    def with_includes(self, includes: list[str]) -> Go:
        self._includes = includes
        return self

    def with_secrets(self, secrets: dict[str, str]) -> Go:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def with_source(self, source: artifact_pb2.ArtifactSource) -> Go:
        self._source = source
        return self

    def with_source_script(self, script: str) -> Go:
        if script not in self._source_scripts:
            self._source_scripts.append(script)
        return self

    def build(self, context: ConfigContext) -> str:
        _raise_systems_error(self._systems_error)
        source_path = "."
        if self._source is not None:
            source = self._source
        else:
            source_builder = ArtifactSource(self._name, source_path)
            if len(self._includes) > 0:
                source_builder.with_includes(self._includes)
            source = source_builder.build()

        source_dir = f"./source/{source.name}"

        # Built incrementally to match the Rust SDK's formatdoc! concatenation:
        #   1. pushd + mkdir   2. (optional) source scripts   3. go build + clean
        step_script = f"pushd {source_dir}\n\nmkdir -p $VORPAL_OUTPUT/bin"

        if len(self._source_scripts) > 0:
            source_scripts = "\n".join(self._source_scripts)
            step_script = f"{step_script}\n\n{source_scripts}"

        step_script = (
            f"{step_script}\n\n"
            f"go build -C {self._build_directory} "
            f"-o $VORPAL_OUTPUT/bin/{self._name} "
            f"{self._build_flags} {self._build_path}\n\n"
            f"go clean -modcache"
        )

        git = Git().build(context)
        go = GoBin().build(context)

        goarch = get_goarch(context.get_system())
        goos = get_goos(context.get_system())

        step_environments = [
            f"GOARCH={goarch}",
            "GOCACHE=$VORPAL_WORKSPACE/go/cache",
            f"GOOS={goos}",
            "GOPATH=$VORPAL_WORKSPACE/go",
            f"PATH={get_env_key(go)}/bin",
        ]
        step_environments.extend(self._environments)

        step_artifacts = [git, go, *self._artifacts]

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


class GoDevelopmentEnvironment:
    """Builder for Go development-environment artifacts."""

    def __init__(
        self, name: str, systems: Sequence[ArtifactSystemInput]
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems, self._systems_error = _normalize_systems_for_build(
            systems
        )
        self._include_protoc = True
        self._include_protoc_gen_go = True
        self._include_protoc_gen_go_grpc = True

    def with_artifacts(self, artifacts: list[str]) -> GoDevelopmentEnvironment:
        self._artifacts.extend(artifacts)
        return self

    def with_environments(
        self, environments: list[str]
    ) -> GoDevelopmentEnvironment:
        self._environments.extend(environments)
        return self

    def without_protoc(self) -> GoDevelopmentEnvironment:
        self._include_protoc = False
        self._include_protoc_gen_go = False
        self._include_protoc_gen_go_grpc = False
        return self

    def with_secrets(
        self, secrets: dict[str, str]
    ) -> GoDevelopmentEnvironment:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: ConfigContext) -> str:
        _raise_systems_error(self._systems_error)
        go = GoBin().build(context)
        git = Git().build(context)
        goimports = Goimports().build(context)
        gopls = Gopls().build(context)
        staticcheck = Staticcheck().build(context)

        artifacts = [git, go, goimports, gopls]

        if self._include_protoc:
            artifacts.append(Protoc().build(context))
        if self._include_protoc_gen_go:
            artifacts.append(ProtocGenGo().build(context))
        if self._include_protoc_gen_go_grpc:
            artifacts.append(ProtocGenGoGrpc().build(context))

        artifacts.append(staticcheck)
        artifacts.extend(self._artifacts)

        goarch = get_goarch(context.get_system())
        goos = get_goos(context.get_system())

        environments = [
            "CGO_ENABLED=0",
            f"GOARCH={goarch}",
            f"GOOS={goos}",
            *self._environments,
        ]

        devenv = (
            DevelopmentEnvironment(self._name, self._systems)
            .with_artifacts(artifacts)
            .with_environments(environments)
        )

        if len(self._secrets) > 0:
            devenv = devenv.with_secrets(self._secrets)

        return devenv.build(context)
