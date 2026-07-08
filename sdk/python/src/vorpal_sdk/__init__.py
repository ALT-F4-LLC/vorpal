# Public surface of the vorpal_sdk package — mirrors sdk/typescript/src/index.ts.
#
# All tool/language re-exports live here (not in artifact/__init__.py): by the time
# this module runs, artifact/__init__.py is fully initialized, so the tool builders'
# `from vorpal_sdk.artifact import Artifact` resolves cleanly. Putting the re-exports
# inside artifact/__init__.py would create an init->tool->init cycle.
#
# Authors should import from this top-level namespace, not from submodules.

from importlib.metadata import version

from vorpal_sdk.api.artifact.artifact_pb2 import ArtifactStepSecret, ArtifactSystem
from vorpal_sdk.artifact import (
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
from vorpal_sdk.artifact.cpython import (
    DEFAULT_PYTHON_VERSION,
    Cpython,
    cpython_target,
)
from vorpal_sdk.artifact.gh import Gh
from vorpal_sdk.artifact.go import GoBin
from vorpal_sdk.artifact.go import source_tools as go_source_tools
from vorpal_sdk.artifact.language.go import Go, GoDevelopmentEnvironment
from vorpal_sdk.artifact.language.python import (
    Python,
    PythonDevelopmentEnvironment,
)
from vorpal_sdk.artifact.language.rust import Rust, RustDevelopmentEnvironment
from vorpal_sdk.artifact.language.typescript import (
    TypeScript,
    TypeScriptDevelopmentEnvironment,
)
from vorpal_sdk.artifact.nodejs import NodeJS
from vorpal_sdk.artifact.protoc import Protoc
from vorpal_sdk.artifact.uv import DEFAULT_UV_VERSION, Uv
from vorpal_sdk.cli import StartCommand, parse_cli_args
from vorpal_sdk.context import (
    ArtifactAlias,
    ConfigContext,
    format_artifact_alias,
    parse_artifact_alias,
)
from vorpal_sdk.step import bash, bwrap, docker, shell
from vorpal_sdk.system import (
    ArtifactSystemInput,
    get_system,
    get_system_default,
    get_system_default_str,
    get_system_str,
    normalize_systems,
)

__version__ = version("vorpal-sdk")

__all__: list[str] = [
    "__version__",
    # Core artifact builders
    "Artifact",
    "ArtifactSource",
    "ArtifactStep",
    "Argument",
    "Job",
    "OciImage",
    "Process",
    "DevelopmentEnvironment",
    "UserEnvironment",
    "get_env_key",
    "secrets_to_proto",
    # Go distribution + shared Go-tools source helper
    "GoBin",
    "go_source_tools",
    # CPython interpreter
    "Cpython",
    "DEFAULT_PYTHON_VERSION",
    "cpython_target",
    # uv toolchain
    "Uv",
    "DEFAULT_UV_VERSION",
    # Node.js runtime
    "NodeJS",
    # GitHub CLI
    "Gh",
    # protoc
    "Protoc",
    # Step functions
    "bash",
    "bwrap",
    "shell",
    "docker",
    # Language builders
    "Go",
    "Python",
    "Rust",
    "TypeScript",
    # Development environment builders
    "GoDevelopmentEnvironment",
    "PythonDevelopmentEnvironment",
    "RustDevelopmentEnvironment",
    "TypeScriptDevelopmentEnvironment",
    # System utilities
    "ArtifactSystemInput",
    "get_system",
    "get_system_default",
    "get_system_default_str",
    "get_system_str",
    "normalize_systems",
    # Context
    "ConfigContext",
    "ArtifactAlias",
    "format_artifact_alias",
    "parse_artifact_alias",
    # CLI
    "parse_cli_args",
    "StartCommand",
    # Commonly used generated types
    "ArtifactSystem",
    "ArtifactStepSecret",
]
