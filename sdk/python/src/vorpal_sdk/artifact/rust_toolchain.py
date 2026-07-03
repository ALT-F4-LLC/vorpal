"""Unified Rust toolchain artifact (assembles the 7 sub-components).

Mirrors ``sdk/typescript/src/artifact/rust_toolchain.js``.

This module is the canonical home for ``RUST_TOOLCHAIN_VERSION`` and
``rust_toolchain_target`` (the TS SDK splits these between rust_toolchain.ts and
language/rust.ts and re-exports across the boundary — a cycle that ESM tolerates but
Python does not). Defining them here, with no upward imports, lets the 7 sub-component
builders and ``language/rust`` depend on this module one-directionally. The sub-builder
imports inside ``build`` are deferred for the same reason: those modules import the
constant/target from here at module load.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import Artifact, get_env_key
from vorpal_sdk.step import shell

if TYPE_CHECKING:
    from vorpal_sdk.context import ConfigContext

RUST_TOOLCHAIN_VERSION = "1.93.1"


def rust_toolchain_target(system: artifact_pb2.ArtifactSystem) -> str:
    """Map an ArtifactSystem to the Rust target triple.

    Matches Go ``RustToolchainTarget()`` in rust_toolchain.go.
    """
    if system == artifact_pb2.AARCH64_DARWIN:
        return "aarch64-apple-darwin"
    if system == artifact_pb2.AARCH64_LINUX:
        return "aarch64-unknown-linux-gnu"
    if system == artifact_pb2.X8664_DARWIN:
        return "x86_64-apple-darwin"
    if system == artifact_pb2.X8664_LINUX:
        return "x86_64-unknown-linux-gnu"
    raise ValueError(f"unsupported 'rust-toolchain' system: {system}")


class RustToolchain:
    """Builder for the unified Rust toolchain artifact.

    Assembles cargo, clippy, rust-analyzer, rust-src, rust-std, rustc, and rustfmt
    into a single toolchain directory. Each component may be supplied explicitly via a
    ``with_*`` setter; otherwise it is built from its default builder.
    """

    def __init__(self) -> None:
        self._cargo: str | None = None
        self._clippy: str | None = None
        self._rust_analyzer: str | None = None
        self._rust_src: str | None = None
        self._rust_std: str | None = None
        self._rustc: str | None = None
        self._rustfmt: str | None = None

    def with_cargo(self, cargo: str) -> RustToolchain:
        self._cargo = cargo
        return self

    def with_clippy(self, clippy: str) -> RustToolchain:
        self._clippy = clippy
        return self

    def with_rust_analyzer(self, rust_analyzer: str) -> RustToolchain:
        self._rust_analyzer = rust_analyzer
        return self

    def with_rust_src(self, rust_src: str) -> RustToolchain:
        self._rust_src = rust_src
        return self

    def with_rust_std(self, rust_std: str) -> RustToolchain:
        self._rust_std = rust_std
        return self

    def with_rustc(self, rustc: str) -> RustToolchain:
        self._rustc = rustc
        return self

    def with_rustfmt(self, rustfmt: str) -> RustToolchain:
        self._rustfmt = rustfmt
        return self

    def build(self, context: ConfigContext) -> str:
        # Deferred imports break the sub-component<->toolchain cycle: cargo.py et al.
        # import RUST_TOOLCHAIN_VERSION/rust_toolchain_target from this module.
        from vorpal_sdk.artifact.cargo import Cargo
        from vorpal_sdk.artifact.clippy import Clippy
        from vorpal_sdk.artifact.rust_analyzer import RustAnalyzer
        from vorpal_sdk.artifact.rust_src import RustSrc
        from vorpal_sdk.artifact.rust_std import RustStd
        from vorpal_sdk.artifact.rustc import Rustc
        from vorpal_sdk.artifact.rustfmt import Rustfmt

        cargo = self._cargo if self._cargo is not None else Cargo().build(context)
        clippy = (
            self._clippy if self._clippy is not None else Clippy().build(context)
        )
        rust_analyzer = (
            self._rust_analyzer
            if self._rust_analyzer is not None
            else RustAnalyzer().build(context)
        )
        rust_src = (
            self._rust_src
            if self._rust_src is not None
            else RustSrc().build(context)
        )
        rust_std = (
            self._rust_std
            if self._rust_std is not None
            else RustStd().build(context)
        )
        rustc = self._rustc if self._rustc is not None else Rustc().build(context)
        rustfmt = (
            self._rustfmt
            if self._rustfmt is not None
            else Rustfmt().build(context)
        )

        artifacts = [
            cargo,
            clippy,
            rust_analyzer,
            rust_src,
            rust_std,
            rustc,
            rustfmt,
        ]

        component_paths = " ".join(get_env_key(a) for a in artifacts)

        toolchain_target = rust_toolchain_target(context.get_system())
        toolchain_version = RUST_TOOLCHAIN_VERSION

        toolchain_dir_line = (
            f'toolchain_dir="$VORPAL_OUTPUT/toolchains/'
            f'{toolchain_version}-{toolchain_target}"'
        )

        step_script = f"""{toolchain_dir_line}

mkdir -p "$toolchain_dir"

components=({component_paths})

echo "Copying Rust toolchain components to $toolchain_dir..."

for component in "${{components[@]}}"; do
    echo "Processing component: $component"

    find "$component" | while read -r file; do
        relative_path=$(echo "$file" | sed -e "s|$component||")

        if [[ "$relative_path" == "/manifest.in" ]]; then
            continue
        fi

        if [ -d "$file" ]; then
            mkdir -p "$toolchain_dir$relative_path"
        else
            cp -p "$file" "$toolchain_dir$relative_path"
        fi
    done
done

cat > "$VORPAL_OUTPUT/settings.toml" << "EOF"
auto_self_update = "disable"
profile = "minimal"
version = "12"

[overrides]
EOF"""

        steps = [shell(context, artifacts, [], step_script, [])]
        systems = [
            artifact_pb2.AARCH64_DARWIN,
            artifact_pb2.AARCH64_LINUX,
            artifact_pb2.X8664_DARWIN,
            artifact_pb2.X8664_LINUX,
        ]
        name = "rust-toolchain"

        return (
            Artifact(name, steps, systems)
            .with_aliases([f"{name}:{toolchain_version}"])
            .build(context)
        )
