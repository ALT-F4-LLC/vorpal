"""Config entry point — parses CLI args, dispatches on the artifact name, and
runs the ``ContextService``.

Mirrors the structure of ``sdk/typescript/src/vorpal.ts`` and the canonical
``config/src/main.rs``: build the
:class:`~vorpal_sdk.context.ConfigContext`, register the requested artifact's
build graph, then serve the context until SIGINT/SIGTERM. Each ``build_*``
function mirrors its Rust/TS counterpart 1:1 for cross-SDK digest parity.
"""

from __future__ import annotations

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.artifact import (
    Argument,
    Artifact,
    ArtifactSource,
    DevelopmentEnvironment,
    Job,
    OciImage,
    Process,
    UserEnvironment,
    get_env_key,
)
from vorpal_sdk.artifact.bun import Bun
from vorpal_sdk.artifact.crane import Crane
from vorpal_sdk.artifact.gh import Gh
from vorpal_sdk.artifact.go import GoBin
from vorpal_sdk.artifact.goimports import Goimports
from vorpal_sdk.artifact.gopls import Gopls
from vorpal_sdk.artifact.grpcurl import Grpcurl
from vorpal_sdk.artifact.language.go import get_goarch, get_goos
from vorpal_sdk.artifact.language.rust import Rust
from vorpal_sdk.artifact.nodejs import NodeJS
from vorpal_sdk.artifact.pnpm import Pnpm
from vorpal_sdk.artifact.protoc import Protoc
from vorpal_sdk.artifact.protoc_gen_go import ProtocGenGo
from vorpal_sdk.artifact.protoc_gen_go_grpc import ProtocGenGoGrpc
from vorpal_sdk.artifact.rsync import Rsync
from vorpal_sdk.artifact.staticcheck import Staticcheck
from vorpal_sdk.context import ConfigContext
from vorpal_sdk.step import shell

SYSTEMS = [
    artifact_pb2.AARCH64_DARWIN,
    artifact_pb2.AARCH64_LINUX,
    artifact_pb2.X8664_DARWIN,
    artifact_pb2.X8664_LINUX,
]


def build_vorpal(context: ConfigContext) -> str:
    return (
        Rust("vorpal", SYSTEMS)
        .with_bins(["vorpal"])
        .with_includes(["cli", "sdk/rust"])
        .with_packages(["vorpal-cli", "vorpal-sdk"])
        .build(context)
    )


def build_vorpal_container_image(context: ConfigContext) -> str:
    linux_vorpal_slim = context.fetch_artifact_alias("linux-vorpal-slim:latest")
    vorpal = build_vorpal(context)

    name = "vorpal-container-image"

    return (
        OciImage(name, linux_vorpal_slim)
        .with_aliases([f"{name}:latest"])
        .with_artifacts([vorpal])
        .with_crane(Crane().build(context))
        .with_rsync(Rsync().build(context))
        .build(context)
    )


def build_vorpal_job(context: ConfigContext) -> str:
    vorpal = build_vorpal(context)
    script = f"{get_env_key(vorpal)}/bin/vorpal --version"

    return Job("vorpal-job", script, SYSTEMS).with_artifacts([vorpal]).build(context)


def build_vorpal_process(context: ConfigContext) -> str:
    vorpal = build_vorpal(context)

    return (
        Process(
            "vorpal-process",
            f"{get_env_key(vorpal)}/bin/vorpal",
            SYSTEMS,
        )
        .with_arguments(
            [
                "--registry",
                "https://localhost:50051",
                "services",
                "start",
                "--port",
                "50051",
            ]
        )
        .with_artifacts([vorpal])
        .build(context)
    )


def build_vorpal_release(context: ConfigContext) -> str:
    branch_name = Argument("branch-name").with_require().build(context)
    darwin_aarch64 = Argument("aarch64-darwin").with_require().build(context)
    darwin_x8664 = Argument("x8664-darwin").with_require().build(context)
    linux_aarch64 = Argument("aarch64-linux").with_require().build(context)
    linux_x8664 = Argument("x8664-linux").with_require().build(context)

    aarch64_darwin = context.fetch_artifact(darwin_aarch64)
    aarch64_linux = context.fetch_artifact(linux_aarch64)
    x8664_darwin = context.fetch_artifact(darwin_x8664)
    x8664_linux = context.fetch_artifact(linux_x8664)

    script = f"""git clone \\
    --branch {branch_name} \\
    --depth 1 \\
        git@github.com:ALT-F4-LLC/vorpal.git

    pushd vorpal

    git fetch --tags
    git tag --delete nightly || true
    git push origin :refs/tags/nightly || true
    gh release delete --yes nightly || true

    git tag nightly
    git push --tags

    gh release create \\
    --notes "Nightly builds from main branch." \\
    --prerelease \\
    --title "nightly" \\
    --verify-tag \\
    nightly \\
    {get_env_key(aarch64_darwin)}.tar.zst \\
    {get_env_key(aarch64_linux)}.tar.zst \\
    {get_env_key(x8664_darwin)}.tar.zst \\
    {get_env_key(x8664_linux)}.tar.zst"""

    gh = Gh().build(context)

    return (
        Job("vorpal-release", script, SYSTEMS)
        .with_artifacts(
            [
                aarch64_darwin,
                aarch64_linux,
                gh,
                x8664_darwin,
                x8664_linux,
            ]
        )
        .build(context)
    )


def build_vorpal_shell(context: ConfigContext) -> str:
    bun = Bun().build(context)
    crane = Crane().build(context)
    gh = Gh().build(context)
    go = GoBin().build(context)
    goimports = Goimports().build(context)
    gopls = Gopls().build(context)
    grpcurl = Grpcurl().build(context)
    nodejs = NodeJS().build(context)
    pnpm = Pnpm().build(context)
    protoc = Protoc().build(context)
    protoc_gen_go = ProtocGenGo().build(context)
    protoc_gen_go_grpc = ProtocGenGoGrpc().build(context)
    rsync = Rsync().build(context)
    staticcheck = Staticcheck().build(context)

    goarch = get_goarch(context.get_system())
    goos = get_goos(context.get_system())

    return (
        DevelopmentEnvironment("vorpal-shell", SYSTEMS)
        .with_artifacts(
            [
                bun,
                crane,
                gh,
                go,
                goimports,
                gopls,
                grpcurl,
                nodejs,
                pnpm,
                protoc,
                protoc_gen_go,
                protoc_gen_go_grpc,
                rsync,
                staticcheck,
            ]
        )
        .with_environments(
            [
                "CGO_ENABLED=0",
                f"GOARCH={goarch}",
                f"GOOS={goos}",
            ]
        )
        .build(context)
    )


def build_vorpal_user(context: ConfigContext) -> str:
    return (
        UserEnvironment("vorpal-user", SYSTEMS)
        .with_artifacts([])
        .with_environments(["PATH=$HOME/.vorpal/bin"])
        .with_symlinks(
            [
                (
                    "$HOME/Development/repository/github.com/ALT-F4-LLC/vorpal.git/main/target/debug/vorpal",
                    "$HOME/.vorpal/bin/vorpal",
                ),
            ]
        )
        .build(context)
    )


def build_vorpal_website(context: ConfigContext) -> str:
    bun = Bun().build(context)
    bun_bin = f"{get_env_key(bun)}/bin"

    name = "vorpal-website"

    source = (
        ArtifactSource(name, ".")
        .with_includes(["website"])
        .with_excludes(
            [
                "website/.astro",
                "website/README.md",
                "website/dist",
                "website/node_modules",
            ]
        )
        .build()
    )

    step_script = f"""pushd ./source/vorpal-website/website
{bun_bin}/bun install
{bun_bin}/bun run build
cp -r dist/* $VORPAL_OUTPUT/
"""

    steps = [
        shell(
            context,
            [bun],
            [
                "ASTRO_TELEMETRY_DISABLED=1",
                f"PATH={bun_bin}",
            ],
            step_script,
            [],
        ),
    ]

    return Artifact(name, steps, SYSTEMS).with_sources([source]).build(context)


def main() -> None:
    context = ConfigContext.create()

    name = context.get_artifact_name()

    if name == "vorpal":
        build_vorpal(context)
    elif name == "vorpal-container-image":
        build_vorpal_container_image(context)
    elif name == "vorpal-job":
        build_vorpal_job(context)
    elif name == "vorpal-process":
        build_vorpal_process(context)
    elif name == "vorpal-release":
        build_vorpal_release(context)
    elif name == "vorpal-shell":
        build_vorpal_shell(context)
    elif name == "vorpal-user":
        build_vorpal_user(context)
    elif name == "vorpal-website":
        build_vorpal_website(context)
    else:
        raise ValueError(f"unknown artifact: {name}")

    context.run()


if __name__ == "__main__":
    main()
