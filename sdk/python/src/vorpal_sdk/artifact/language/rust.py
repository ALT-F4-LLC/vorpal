"""Rust/Cargo project + development-environment artifact builders.

Mirrors ``sdk/typescript/src/artifact/language/rust.js``. The vendor + build
scripts must be IDENTICAL to the Rust and Go SDKs: both always emit
``if [ "false"/"true" = "true" ]`` blocks for every option (format, lint, check,
build, tests), even when disabled, so artifact digests match.
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import (
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    get_env_key,
    secrets_to_proto,
)
from vorpal_sdk.artifact.protoc import Protoc
from vorpal_sdk.artifact.rust_toolchain import (
    RUST_TOOLCHAIN_VERSION,
    RustToolchain,
    rust_toolchain_target,
)
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext


# ---------------------------------------------------------------------------
# Cargo.toml parsing
# ---------------------------------------------------------------------------


@dataclass
class _CargoBinary:
    name: str
    path: str


@dataclass
class _CargoToml:
    bin: list[_CargoBinary] = field(default_factory=list)
    package_name: str | None = None
    workspace_members: list[str] | None = None


def parse_cargo(path: str) -> _CargoToml:
    """Parse a Cargo.toml with stdlib tomllib, extracting the fields the Rust
    builder needs: ``[package]`` name, ``[workspace]`` members, ``[[bin]]`` entries.
    """
    with open(path, "rb") as handle:
        doc = tomllib.load(handle)

    result = _CargoToml()

    package = doc.get("package")
    if isinstance(package, dict) and isinstance(package.get("name"), str):
        result.package_name = package["name"]

    workspace = doc.get("workspace")
    if isinstance(workspace, dict) and isinstance(workspace.get("members"), list):
        result.workspace_members = [
            m for m in workspace["members"] if isinstance(m, str)
        ]

    bins = doc.get("bin")
    if isinstance(bins, list):
        for entry in bins:
            if (
                isinstance(entry, dict)
                and isinstance(entry.get("name"), str)
                and isinstance(entry.get("path"), str)
            ):
                result.bin.append(_CargoBinary(entry["name"], entry["path"]))

    return result


# ---------------------------------------------------------------------------
# Shell script helpers
# ---------------------------------------------------------------------------


def _build_vendor_script(
    name: str, packages: list[str], packages_targets: list[str]
) -> str:
    """Build the vendor step script for ``cargo vendor``."""
    lines: list[str] = []

    lines.append("mkdir -p $HOME")
    lines.append("")
    lines.append(f"pushd ./source/{name}-vendor")

    if len(packages) > 0:
        quoted_packages = ",".join(f'"{p}"' for p in packages)
        quoted_targets = " ".join(f'"{t}"' for t in packages_targets)

        lines.append("")
        lines.append('cat > Cargo.toml << "EOF"')
        lines.append("[workspace]")
        lines.append(f"members = [{quoted_packages}]")
        lines.append('resolver = "2"')
        lines.append("EOF")

        lines.append("")
        lines.append(f"target_paths=({quoted_targets})")
        lines.append("")
        lines.append("for target_path in ${target_paths[@]}; do")
        lines.append("    mkdir -p $(dirname ${target_path})")
        lines.append("    touch ${target_path}")
        lines.append("done")
    else:
        lines.append("")
        lines.append("mkdir -p src")
        lines.append("touch src/main.rs")

    lines.append("")
    lines.append("mkdir -p $VORPAL_OUTPUT/vendor")
    lines.append("")
    lines.append(
        "cargo_vendor=$(cargo vendor --versioned-dirs $VORPAL_OUTPUT/vendor)"
    )
    lines.append("")
    lines.append('echo "$cargo_vendor" > $VORPAL_OUTPUT/config.toml')

    return "\n".join(lines)


def _build_main_script(
    *,
    name: str,
    vendor_digest: str,
    packages: list[str],
    bin_names: list[str],
    manifests: list[str],
    fmt: bool,
    lint: bool,
    check: bool,
    build: bool,
    tests: bool,
) -> str:
    """Build the main build step script for ``cargo build --release``.

    Emits the ``if [ "..." = "true" ]`` conditional block for every option even
    when disabled, matching Rust formatdoc! / Go StepScriptTemplate exactly.
    """
    enable_format = "true" if fmt else "false"
    enable_lint = "true" if lint else "false"
    enable_check = "true" if check else "false"
    enable_build = "true" if build else "false"
    enable_tests = "true" if tests else "false"

    lines: list[str] = []

    lines.append("mkdir -p $HOME")
    lines.append("")
    lines.append(f"pushd ./source/{name}")
    lines.append("")
    lines.append("mkdir -p .cargo")
    lines.append("mkdir -p $VORPAL_OUTPUT/bin")
    lines.append("")
    lines.append(
        f"ln -s {get_env_key(vendor_digest)}/config.toml .cargo/config.toml"
    )

    if len(packages) > 0:
        quoted_packages = ",".join(f'"{p}"' for p in packages)
        lines.append("")
        lines.append('cat > Cargo.toml << "EOF"')
        lines.append("[workspace]")
        lines.append(f"members = [{quoted_packages}]")
        lines.append('resolver = "2"')
        lines.append("EOF")

    lines.append("")
    lines.append(f"bin_names=({' '.join(bin_names)})")
    lines.append(f"manifest_paths=({' '.join(manifests)})")
    lines.append("")
    lines.append(f'if [ "{enable_format}" = "true" ]; then')
    lines.append('    echo "Running formatter..."')
    lines.append("    cargo --offline fmt --all --check")
    lines.append("fi")
    lines.append("")
    lines.append("for manifest_path in ${manifest_paths[@]}; do")
    lines.append(f'    if [ "{enable_lint}" = "true" ]; then')
    lines.append('        echo "Running linter..."')
    lines.append(
        "        cargo --offline clippy --manifest-path ${manifest_path} "
        "-- --deny warnings"
    )
    lines.append("    fi")
    lines.append("done")
    lines.append("")
    lines.append("for bin_name in ${bin_names[@]}; do")
    lines.append(f'    if [ "{enable_check}" = "true" ]; then')
    lines.append('        echo "Running check..."')
    lines.append("        cargo --offline check --bin ${bin_name} --release")
    lines.append("    fi")
    lines.append("")
    lines.append(f'    if [ "{enable_build}" = "true" ]; then')
    lines.append('        echo "Running build..."')
    lines.append("        cargo --offline build --bin ${bin_name} --release")
    lines.append("    fi")
    lines.append("")
    lines.append(f'    if [ "{enable_tests}" = "true" ]; then')
    lines.append('        echo "Running tests..."')
    lines.append("        cargo --offline test --bin ${bin_name} --release")
    lines.append("    fi")
    lines.append("")
    lines.append("    cp -p ./target/release/${bin_name} $VORPAL_OUTPUT/bin/")
    lines.append("done")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Rust
# ---------------------------------------------------------------------------


class Rust:
    """Builder for Rust/Cargo project artifacts."""

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._aliases: list[str] = []
        self._artifacts: list[str] = []
        self._bins: list[str] = []
        self._build = True
        self._check = False
        self._environments: list[str] = []
        self._excludes: list[str] = []
        self._format = False
        self._includes: list[str] = []
        self._lint = False
        self._name = name
        self._packages: list[str] = []
        self._secrets: dict[str, str] = {}
        self._source: str | None = None
        self._systems = systems
        self._tests = False

    def with_aliases(self, aliases: list[str]) -> Rust:
        self._aliases = aliases
        return self

    def with_artifacts(self, artifacts: list[str]) -> Rust:
        self._artifacts = artifacts
        return self

    def with_bins(self, bins: list[str]) -> Rust:
        self._bins = bins
        return self

    def with_check(self, check: bool) -> Rust:
        self._check = check
        return self

    def with_environments(self, environments: list[str]) -> Rust:
        self._environments = environments
        return self

    def with_excludes(self, excludes: list[str]) -> Rust:
        self._excludes = excludes
        return self

    def with_format(self, fmt: bool) -> Rust:
        self._format = fmt
        return self

    def with_includes(self, includes: list[str]) -> Rust:
        self._includes = includes
        return self

    def with_lint(self, lint: bool) -> Rust:
        self._lint = lint
        return self

    def with_packages(self, packages: list[str]) -> Rust:
        self._packages = packages
        return self

    def with_secrets(self, secrets: dict[str, str]) -> Rust:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def with_source(self, source: str) -> Rust:
        self._source = source
        return self

    def with_tests(self, tests: bool) -> Rust:
        self._tests = tests
        return self

    def build(self, context: ConfigContext) -> str:
        protoc = Protoc().build(context)

        # Raw paths use string concatenation (not os.path.join) to preserve a
        # leading "./" segment, matching Rust PathBuf::join and Go Sprintf — the
        # embedded path string is digest-load-bearing.
        context_path = context.get_artifact_context_path()
        source_path = self._source if self._source is not None else "."
        context_path_source_raw = f"{context_path}/{source_path}"
        context_path_source = os.path.join(context_path, source_path)

        if not os.path.exists(context_path_source):
            raise ValueError(
                f"`source.{self._name}.path` not found: {source_path}"
            )

        source_cargo_path = os.path.join(context_path_source, "Cargo.toml")
        source_cargo_path_raw = f"{context_path_source_raw}/Cargo.toml"

        if not os.path.exists(source_cargo_path):
            raise ValueError(f"Cargo.toml not found: {source_cargo_path}")

        source_cargo = parse_cargo(source_cargo_path)

        packages: list[str] = []
        packages_bin_names: list[str] = []
        packages_manifests: list[str] = []
        packages_targets: list[str] = []

        if (
            source_cargo.workspace_members is not None
            and len(source_cargo.workspace_members) > 0
        ):
            for member in source_cargo.workspace_members:
                package_path = os.path.join(context_path_source, member)
                package_path_raw = f"{context_path_source_raw}/{member}"
                package_cargo_path = os.path.join(package_path, "Cargo.toml")
                package_cargo_path_raw = f"{package_path_raw}/Cargo.toml"

                if not os.path.exists(package_cargo_path):
                    raise ValueError(
                        f"Cargo.toml not found: {package_cargo_path}"
                    )

                package_cargo = parse_cargo(package_cargo_path)

                if (
                    len(self._packages) > 0
                    and package_cargo.package_name is not None
                    and package_cargo.package_name not in self._packages
                ):
                    continue

                package_target_paths: list[str] = []

                if len(package_cargo.bin) > 0:
                    for binary in package_cargo.bin:
                        package_target_path = os.path.join(
                            package_path, binary.path
                        )

                        if not os.path.exists(package_target_path):
                            raise ValueError(
                                f"bin target not found: {package_target_path}"
                            )

                        package_target_paths.append(package_target_path)

                        if (
                            len(self._bins) == 0
                            or binary.name in self._bins
                        ):
                            if (
                                package_cargo_path_raw
                                not in packages_manifests
                            ):
                                packages_manifests.append(
                                    package_cargo_path_raw
                                )
                            packages_bin_names.append(binary.name)

                if len(package_target_paths) == 0:
                    package_target_path = os.path.join(
                        package_path, "src/lib.rs"
                    )
                    if not os.path.exists(package_target_path):
                        raise ValueError(
                            f"lib.rs not found: {package_target_path}"
                        )
                    package_target_paths.append(package_target_path)

                for member_target_path in package_target_paths:
                    member_target_path_relative = os.path.relpath(
                        member_target_path, context_path_source
                    )
                    packages_targets.append(member_target_path_relative)

                packages.append(member)

        rust_toolchain = RustToolchain().build(context)

        rust_toolchain_target_str = rust_toolchain_target(context.get_system())
        rust_toolchain_name = (
            f"{RUST_TOOLCHAIN_VERSION}-{rust_toolchain_target_str}"
        )

        step_environments = [
            "HOME=$VORPAL_WORKSPACE/home",
            f"PATH={get_env_key(rust_toolchain)}"
            f"/toolchains/{rust_toolchain_name}/bin",
            f"RUSTUP_HOME={get_env_key(rust_toolchain)}",
            f"RUSTUP_TOOLCHAIN={rust_toolchain_name}",
            *self._environments,
        ]

        vendor_cargo_paths = ["Cargo.toml", "Cargo.lock"]
        for pkg in packages:
            vendor_cargo_paths.append(f"{pkg}/Cargo.toml")

        vendor_step_script = _build_vendor_script(
            self._name, packages, packages_targets
        )

        proto_secrets = secrets_to_proto(self._secrets)

        vendor_step = shell(
            context,
            [rust_toolchain],
            step_environments,
            vendor_step_script,
            proto_secrets,
        )

        vendor_name = f"{self._name}-vendor"

        vendor_source = (
            ArtifactSource(vendor_name, source_path)
            .with_includes(vendor_cargo_paths)
            .build()
        )

        vendor = (
            Artifact(vendor_name, [vendor_step], self._systems)
            .with_sources([vendor_source])
            .build(context)
        )

        step_artifacts = [rust_toolchain, vendor, protoc]

        source_excludes = ["target", *self._excludes]
        source_includes = [*self._includes]

        source = (
            ArtifactSource(self._name, source_path)
            .with_includes(source_includes)
            .with_excludes(source_excludes)
            .build()
        )

        step_artifacts.extend(self._artifacts)

        if len(packages_bin_names) == 0:
            packages_bin_names.append(self._name)

        if len(packages_manifests) == 0:
            packages_manifests.append(source_cargo_path_raw)

        step_script = _build_main_script(
            name=self._name,
            vendor_digest=vendor,
            packages=packages,
            bin_names=packages_bin_names,
            manifests=packages_manifests,
            fmt=self._format,
            lint=self._lint,
            check=self._check,
            build=self._build,
            tests=self._tests,
        )

        step = shell(
            context,
            step_artifacts,
            step_environments,
            step_script,
            proto_secrets,
        )

        return (
            Artifact(self._name, [step], self._systems)
            .with_aliases(self._aliases)
            .with_sources([source])
            .build(context)
        )


# ---------------------------------------------------------------------------
# Rust Development Environment
# ---------------------------------------------------------------------------


class RustDevelopmentEnvironment:
    """Builder for Rust development-environment artifacts."""

    def __init__(
        self, name: str, systems: list[artifact_pb2.ArtifactSystem]
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems = systems
        self._include_protoc = True

    def with_artifacts(
        self, artifacts: list[str]
    ) -> RustDevelopmentEnvironment:
        self._artifacts.extend(artifacts)
        return self

    def with_environments(
        self, environments: list[str]
    ) -> RustDevelopmentEnvironment:
        self._environments.extend(environments)
        return self

    def without_protoc(self) -> RustDevelopmentEnvironment:
        self._include_protoc = False
        return self

    def with_secrets(
        self, secrets: dict[str, str]
    ) -> RustDevelopmentEnvironment:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: ConfigContext) -> str:
        rust_toolchain = RustToolchain().build(context)

        artifacts: list[str] = []

        if self._include_protoc:
            artifacts.append(Protoc().build(context))

        artifacts.append(rust_toolchain)
        artifacts.extend(self._artifacts)

        toolchain_target = rust_toolchain_target(context.get_system())
        toolchain_name = f"{RUST_TOOLCHAIN_VERSION}-{toolchain_target}"
        toolchain_bin = (
            f"{get_env_key(rust_toolchain)}/toolchains/{toolchain_name}/bin"
        )

        environments = [
            f"PATH={toolchain_bin}",
            f"RUSTUP_HOME={get_env_key(rust_toolchain)}",
            f"RUSTUP_TOOLCHAIN={toolchain_name}",
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
