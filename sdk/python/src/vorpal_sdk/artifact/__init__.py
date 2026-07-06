"""Core artifact builders.

Mirrors ``sdk/typescript/src/artifact.ts`` (the canonical reference) and
``sdk/go/pkg/artifact/builder.go``. Sync model — ``build()`` is synchronous,
unlike the TS SDK's async builders.

Each builder is a pure function of its inputs: the produced ``Artifact``
message (and therefore its cross-SDK digest) depends only on the inputs and the
target system. ``build()`` delegates digest computation + registration to the
``BuildContext`` (the Phase-5 ``ConfigContext``); see ``tests/test_builder_parity.py``
for the within-SDK-family builder-output parity gate.
"""

from __future__ import annotations

from vorpal_sdk.api.artifact import artifact_pb2
from vorpal_sdk.step import BuildContext, shell

__all__ = [
    "Argument",
    "Artifact",
    "ArtifactSource",
    "ArtifactStep",
    "DevelopmentEnvironment",
    "Job",
    "OciImage",
    "Process",
    "UserEnvironment",
    "get_env_key",
    "secrets_to_proto",
]


def get_env_key(digest: str) -> str:
    """Return the environment-variable key for an artifact digest.

    Matches Rust ``get_env_key()`` and Go ``GetEnvKey()``.
    """
    return f"$VORPAL_ARTIFACT_{digest}"


def secrets_to_proto(
    secrets: dict[str, str],
) -> list[artifact_pb2.ArtifactStepSecret]:
    """Convert a mapping of secrets to a name-sorted list of proto objects.

    Matches Go ``SecretsToProto`` / TS ``secretsToProto`` — the sort is
    load-bearing for cross-SDK digest parity.
    """
    return [
        artifact_pb2.ArtifactStepSecret(name=name, value=secrets[name])
        for name in sorted(secrets)
    ]


# ---------------------------------------------------------------------------
# ArtifactSource
# ---------------------------------------------------------------------------


class ArtifactSource:
    """Builder for ``ArtifactSource`` messages."""

    def __init__(self, name: str, path: str) -> None:
        self._digest: str | None = None
        self._excludes: list[str] = []
        self._includes: list[str] = []
        self._name = name
        self._path = path

    def with_digest(self, digest: str) -> ArtifactSource:
        self._digest = digest
        return self

    def with_excludes(self, excludes: list[str]) -> ArtifactSource:
        self._excludes = excludes
        return self

    def with_includes(self, includes: list[str]) -> ArtifactSource:
        self._includes = includes
        return self

    def build(self) -> artifact_pb2.ArtifactSource:
        source = artifact_pb2.ArtifactSource(
            excludes=self._excludes,
            includes=self._includes,
            name=self._name,
            path=self._path,
        )
        # ``digest`` is a proto3 optional: set it only when provided so an
        # absent digest serializes as null (not "").
        if self._digest is not None:
            source.digest = self._digest
        return source


# ---------------------------------------------------------------------------
# ArtifactStep
# ---------------------------------------------------------------------------


class ArtifactStep:
    """Builder for ``ArtifactStep`` messages."""

    def __init__(self, entrypoint: str) -> None:
        self._arguments: list[str] = []
        self._artifacts: list[str] = []
        self._entrypoint = entrypoint
        self._environments: list[str] = []
        self._secrets: list[artifact_pb2.ArtifactStepSecret] = []
        self._script: str | None = None

    def with_arguments(self, arguments: list[str]) -> ArtifactStep:
        self._arguments = arguments
        return self

    def with_artifacts(self, artifacts: list[str]) -> ArtifactStep:
        self._artifacts = artifacts
        return self

    def with_environments(self, environments: list[str]) -> ArtifactStep:
        self._environments = environments
        return self

    def with_secrets(
        self, secrets: list[artifact_pb2.ArtifactStepSecret]
    ) -> ArtifactStep:
        existing = {s.name for s in self._secrets}
        for secret in secrets:
            if secret.name not in existing:
                existing.add(secret.name)
                self._secrets.append(secret)
        return self

    def with_script(self, script: str) -> ArtifactStep:
        self._script = script
        return self

    def build(self) -> artifact_pb2.ArtifactStep:
        step = artifact_pb2.ArtifactStep(
            entrypoint=self._entrypoint,
            secrets=self._secrets,
            arguments=self._arguments,
            artifacts=self._artifacts,
            environments=self._environments,
        )
        if self._script is not None:
            step.script = self._script
        return step


# ---------------------------------------------------------------------------
# Artifact
# ---------------------------------------------------------------------------


class Artifact:
    """Builder for ``Artifact`` messages."""

    def __init__(
        self,
        name: str,
        steps: list[artifact_pb2.ArtifactStep],
        systems: list[artifact_pb2.ArtifactSystem],
    ) -> None:
        self._aliases: list[str] = []
        self._name = name
        self._sources: list[artifact_pb2.ArtifactSource] = []
        self._steps = steps
        self._systems = systems

    def with_aliases(self, aliases: list[str]) -> Artifact:
        for alias in aliases:
            if alias not in self._aliases:
                self._aliases.append(alias)
        return self

    def with_sources(
        self, sources: list[artifact_pb2.ArtifactSource]
    ) -> Artifact:
        existing = {s.name for s in self._sources}
        for source in sources:
            if source.name not in existing:
                existing.add(source.name)
                self._sources.append(source)
        return self

    def build(self, context: BuildContext) -> str:
        artifact = artifact_pb2.Artifact(
            target=context.get_system(),
            sources=self._sources,
            steps=self._steps,
            systems=self._systems,
            aliases=self._aliases,
            name=self._name,
        )
        return context.add_artifact(artifact)


# ---------------------------------------------------------------------------
# Job
# ---------------------------------------------------------------------------


class Job:
    """Builder for Job artifacts (simple script execution)."""

    def __init__(
        self,
        name: str,
        script: str,
        systems: list[artifact_pb2.ArtifactSystem],
    ) -> None:
        self._artifacts: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._script = script
        self._systems = systems

    def with_artifacts(self, artifacts: list[str]) -> Job:
        self._artifacts = artifacts
        return self

    def with_secrets(self, secrets: dict[str, str]) -> Job:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: BuildContext) -> str:
        step = shell(
            context,
            self._artifacts,
            [],
            self._script,
            secrets_to_proto(self._secrets),
        )
        return Artifact(self._name, [step], self._systems).build(context)


# ---------------------------------------------------------------------------
# Process
# ---------------------------------------------------------------------------


class Process:
    """Builder for Process artifacts (start/stop/logs helper scripts)."""

    def __init__(
        self,
        name: str,
        entrypoint: str,
        systems: list[artifact_pb2.ArtifactSystem],
    ) -> None:
        self._arguments: list[str] = []
        self._artifacts: list[str] = []
        self._entrypoint = entrypoint
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems = systems

    def with_arguments(self, arguments: list[str]) -> Process:
        self._arguments = arguments
        return self

    def with_artifacts(self, artifacts: list[str]) -> Process:
        for artifact in artifacts:
            if artifact not in self._artifacts:
                self._artifacts.append(artifact)
        return self

    def with_secrets(self, secrets: dict[str, str]) -> Process:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: BuildContext) -> str:
        arguments_str = " ".join(self._arguments)
        artifacts_str = ":".join(
            f"$VORPAL_ARTIFACT_{v}/bin" for v in self._artifacts
        )
        name = self._name
        entrypoint = self._entrypoint

        # Template matches Rust formatdoc! / Go ProcessScriptTemplate exactly.
        script = f"""mkdir -p $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/{name}-logs << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/logs.txt ]; then
    tail -f $VORPAL_OUTPUT/logs.txt
else
    echo "No logs found"
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/{name}-logs

cat > $VORPAL_OUTPUT/bin/{name}-stop << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/pid ]; then
    kill $(cat $VORPAL_OUTPUT/pid)
    rm -rf $VORPAL_OUTPUT/pid
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/{name}-stop

cat > $VORPAL_OUTPUT/bin/{name}-start << "EOF"
#!/bin/bash
set -euo pipefail

export PATH={artifacts_str}:$PATH

$VORPAL_OUTPUT/bin/{name}-stop

echo "Process: {entrypoint} {arguments_str}"

nohup {entrypoint} {arguments_str} > $VORPAL_OUTPUT/logs.txt 2>&1 &

PROCESS_PID=$!

echo "Process ID: $PROCESS_PID"

echo $PROCESS_PID > $VORPAL_OUTPUT/pid

echo "Process commands:"
echo "- {name}-logs (tail logs)"
echo "- {name}-stop (stop process)"
echo "- {name}-start (start process)"
EOF

chmod +x $VORPAL_OUTPUT/bin/{name}-start"""

        step = shell(
            context,
            self._artifacts,
            [],
            script,
            secrets_to_proto(self._secrets),
        )
        return Artifact(self._name, [step], self._systems).build(context)


# ---------------------------------------------------------------------------
# DevelopmentEnvironment
# ---------------------------------------------------------------------------


class DevelopmentEnvironment:
    """Builder for DevelopmentEnvironment artifacts (activate/deactivate)."""

    def __init__(
        self,
        name: str,
        systems: list[artifact_pb2.ArtifactSystem],
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._secrets: dict[str, str] = {}
        self._systems = systems

    def with_artifacts(self, artifacts: list[str]) -> DevelopmentEnvironment:
        self._artifacts = artifacts
        return self

    def with_environments(
        self, environments: list[str]
    ) -> DevelopmentEnvironment:
        self._environments = environments
        return self

    def with_secrets(self, secrets: dict[str, str]) -> DevelopmentEnvironment:
        for key, value in secrets.items():
            if key not in self._secrets:
                self._secrets[key] = value
        return self

    def build(self, context: BuildContext) -> str:
        envs_backup = [
            'export VORPAL_SHELL_BACKUP_PATH="$PATH"',
            'export VORPAL_SHELL_BACKUP_PS1="$PS1"',
            'export VORPAL_SHELL_BACKUP_VORPAL_SHELL="$VORPAL_SHELL"',
        ]
        envs_export = [
            f'export PS1="({self._name}) $PS1"',
            'export VORPAL_SHELL="1"',
        ]
        envs_restore = [
            'export PATH="$VORPAL_SHELL_BACKUP_PATH"',
            'export PS1="$VORPAL_SHELL_BACKUP_PS1"',
            'export VORPAL_SHELL="$VORPAL_SHELL_BACKUP_VORPAL_SHELL"',
        ]
        envs_unset = [
            "unset VORPAL_SHELL_BACKUP_PATH",
            "unset VORPAL_SHELL_BACKUP_PS1",
            "unset VORPAL_SHELL_BACKUP_VORPAL_SHELL",
        ]

        for env in self._environments:
            key = env.split("=")[0]
            if key == "PATH":
                continue
            envs_backup.append(f'export VORPAL_SHELL_BACKUP_{key}="${key}"')
            envs_export.append(f"export {env}")
            envs_restore.append(f'export {key}="$VORPAL_SHELL_BACKUP_{key}"')
            envs_unset.append(f"unset VORPAL_SHELL_BACKUP_{key}")

        step_path = ":".join(
            f"{get_env_key(a)}/bin" for a in self._artifacts
        )
        for env in self._environments:
            if env.startswith("PATH="):
                path_value = "=".join(env.split("=")[1:])
                if path_value:
                    step_path = f"{path_value}:{step_path}"

        envs_export.append(f"export PATH={step_path}:$PATH")

        backups = "\n".join(envs_backup)
        exports = "\n".join(envs_export)
        restores = "\n".join(envs_restore)
        unsets = "\n".join(envs_unset)

        # Template matches Rust formatdoc! / Go template exactly. The literal
        # bash braces in deactivate(){{ }} are doubled for the f-string.
        step_script = f"""mkdir -p $VORPAL_WORKSPACE/bin

cat > bin/activate << "EOF"
#!/bin/bash

{backups}
{exports}

deactivate(){{
{restores}
{unsets}
}}

exec "$@"
EOF

chmod +x $VORPAL_WORKSPACE/bin/activate

mkdir -p $VORPAL_OUTPUT/bin

cp -pr bin "$VORPAL_OUTPUT\""""

        step = shell(
            context,
            self._artifacts,
            [],
            step_script,
            secrets_to_proto(self._secrets),
        )
        return Artifact(self._name, [step], self._systems).build(context)


# ---------------------------------------------------------------------------
# UserEnvironment
# ---------------------------------------------------------------------------


class UserEnvironment:
    """Builder for UserEnvironment artifacts (activate + symlink mgmt)."""

    def __init__(
        self,
        name: str,
        systems: list[artifact_pb2.ArtifactSystem],
    ) -> None:
        self._artifacts: list[str] = []
        self._environments: list[str] = []
        self._name = name
        self._symlinks: list[tuple[str, str]] = []
        self._systems = systems

    def with_artifacts(self, artifacts: list[str]) -> UserEnvironment:
        self._artifacts = artifacts
        return self

    def with_environments(self, environments: list[str]) -> UserEnvironment:
        self._environments = environments
        return self

    def with_symlinks(
        self, symlinks: list[tuple[str, str]]
    ) -> UserEnvironment:
        self._symlinks.extend(symlinks)
        return self

    def build(self, context: BuildContext) -> str:
        # Sort by source path (index 0) for deterministic output.
        symlinks = sorted(self._symlinks, key=lambda pair: pair[0])

        step_path = ":".join(
            f"{get_env_key(a)}/bin" for a in self._artifacts
        )
        for env in self._environments:
            if env.startswith("PATH="):
                path_value = "=".join(env.split("=")[1:])
                if path_value:
                    step_path = f"{path_value}:{step_path}"

        step_environments = "\n".join(
            f"export {e}"
            for e in self._environments
            if not e.startswith("PATH=")
        )
        symlinks_deactivate = "\n".join(
            f"rm -f {target}" for _, target in symlinks
        )
        symlinks_check = "\n".join(
            f'if [ -f {target} ]; then echo "ERROR: Symlink target exists '
            f'-> {target}" && exit 1; fi'
            for _, target in symlinks
        )
        symlinks_activate = "\n".join(
            f"ln -s {source} {target}" for source, target in symlinks
        )

        # Template matches Rust formatdoc! / TS template exactly.
        step_script = f"""mkdir -p $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/vorpal-activate-shell << "EOF"
{step_environments}
export PATH="$VORPAL_OUTPUT/bin:{step_path}:$PATH"
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
{symlinks_deactivate}
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-activate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
{symlinks_check}
{symlinks_activate}
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-activate << "EOF"
#!/bin/bash
set -euo pipefail

echo "Deactivating previous symlinks..."

if [ -f $HOME/.vorpal/bin/vorpal-deactivate-symlinks ]; then
    $HOME/.vorpal/bin/vorpal-deactivate-symlinks
fi

echo "Activating symlinks..."

$VORPAL_OUTPUT/bin/vorpal-activate-symlinks

echo "Vorpal userenv installed. Run 'source vorpal-activate-shell' to activate."

ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-shell $HOME/.vorpal/bin/vorpal-activate-shell
ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-symlinks $HOME/.vorpal/bin/vorpal-activate-symlinks
ln -sf $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks $HOME/.vorpal/bin/vorpal-deactivate-symlinks
EOF


chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-shell
chmod +x $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate"""

        step = shell(context, self._artifacts, [], step_script, [])
        return Artifact(self._name, [step], self._systems).build(context)


# ---------------------------------------------------------------------------
# OciImage
# ---------------------------------------------------------------------------

_OCI_NAME_ALLOWED = set("abcdefghijklmnopqrstuvwxyz0123456789/:-._")


class OciImage:
    """Builder for OCI container-image artifacts (Linux only).

    The default crane/rsync tool builders are not yet ported; supply both via
    :meth:`with_crane` / :meth:`with_rsync` until they land in a later phase.
    """

    def __init__(self, name: str, rootfs: str) -> None:
        self._aliases: list[str] = []
        self._artifacts: list[str] = []
        self._crane: str | None = None
        self._name = name
        self._rootfs = rootfs
        self._rsync: str | None = None

    def with_aliases(self, aliases: list[str]) -> OciImage:
        self._aliases = aliases
        return self

    def with_artifacts(self, artifacts: list[str]) -> OciImage:
        self._artifacts = artifacts
        return self

    def with_crane(self, crane: str) -> OciImage:
        self._crane = crane
        return self

    def with_rsync(self, rsync: str) -> OciImage:
        self._rsync = rsync
        return self

    def build(self, context: BuildContext) -> str:
        if self._name != self._name.lower():
            raise ValueError(
                f"container image name must be lowercase: '{self._name}'"
            )
        for char in self._name:
            if char not in _OCI_NAME_ALLOWED:
                raise ValueError(
                    f"container image name invalid character '{char}': "
                    f"'{self._name}'. Allowed: lowercase letters, digits, "
                    f"and / : - . _"
                )

        if self._crane is None or self._rsync is None:
            raise NotImplementedError(
                "OciImage requires explicit crane + rsync digests "
                "(with_crane/with_rsync); the default tool builders land in a "
                "later phase"
            )
        crane = self._crane
        rsync = self._rsync

        artifacts_list = " ".join(self._artifacts)
        namespace = context.get_artifact_namespace()

        step_script = f"""OCI_IMAGE_ARTIFACTS="{artifacts_list}"
OCI_IMAGE_CRANE="{get_env_key(crane)}"
OCI_IMAGE_NAME="{self._name}"
OCI_IMAGE_ROOTFS="{get_env_key(self._rootfs)}"
OCI_IMAGE_RSYNC="{get_env_key(rsync)}"
OUTPUT_TAR=${{PWD}}/rootfs.tar
ROOTFS_DIR=${{PWD}}/rootfs
STORE_PREFIX=var/lib/vorpal/store/artifact/output/{namespace}

# Detect platform based on build architecture
case "$(uname -m)" in
    x86_64)  OCI_PLATFORM="linux/amd64" ;;
    aarch64) OCI_PLATFORM="linux/arm64" ;;
    *)       OCI_PLATFORM="linux/$(uname -m)" ;;
esac

mkdir -p ${{ROOTFS_DIR}}

for artifact in ${{OCI_IMAGE_ARTIFACTS}}; do
    SOURCE_DIR=/${{STORE_PREFIX}}/${{artifact}}
    TARGET_PATH=${{STORE_PREFIX}}/${{artifact}}

    mkdir -p ${{ROOTFS_DIR}}/${{TARGET_PATH}}

    echo "Copying artifact layer ${{artifact}}..."

    ${{OCI_IMAGE_RSYNC}}/bin/rsync -aW ${{SOURCE_DIR}}/ ${{ROOTFS_DIR}}/${{TARGET_PATH}}

    echo "Copied artifact layer ${{artifact}}"

    # Symlink bin files to /usr/local/bin
    if [ -d "${{SOURCE_DIR}}/bin" ]; then
        mkdir -p ${{ROOTFS_DIR}}/usr/local/bin
        for bin_file in ${{SOURCE_DIR}}/bin/*; do
            if [ -f "${{bin_file}}" ]; then
                bin_name=$(basename "${{bin_file}}")
                ln -sf /${{TARGET_PATH}}/bin/${{bin_name}} ${{ROOTFS_DIR}}/usr/local/bin/${{bin_name}}
                echo "Symlinked ${{bin_name}} to /usr/local/bin"
            fi
        done
    fi
done

echo "Copying Vorpal operating system files..."

${{OCI_IMAGE_RSYNC}}/bin/rsync -aW ${{OCI_IMAGE_ROOTFS}}/ ${{ROOTFS_DIR}}

echo "Copied Vorpal operating system files"

echo "Creating output tarball..."

tar -cf ${{OUTPUT_TAR}} -C ${{ROOTFS_DIR}} .

echo "Created output tarball"

mkdir -p ${{VORPAL_OUTPUT}}

echo "Creating OCI image ${{OCI_IMAGE_NAME}}:latest"

${{OCI_IMAGE_CRANE}}/bin/crane append \\
    --new_layer ${{OUTPUT_TAR}} \\
    --new_tag ${{OCI_IMAGE_NAME}}:latest \\
    --oci-empty-base \\
    --output ${{VORPAL_OUTPUT}}/image.tar \\
    --platform ${{OCI_PLATFORM}}

echo "Setting platform metadata in image config..."

# Extract tarball to modify config (crane mutate cannot work with local files)
WORK_DIR=${{PWD}}/image-work
mkdir -p ${{WORK_DIR}}
tar -xf ${{VORPAL_OUTPUT}}/image.tar -C ${{WORK_DIR}}

# Get config filename from manifest
CONFIG_FILE=$(sed -n 's/.*"Config":"\\([^"]*\\)".*/\\1/p' ${{WORK_DIR}}/manifest.json)

# Detect architecture for config metadata
case "$(uname -m)" in
    x86_64)  CONFIG_ARCH="amd64" ;;
    aarch64) CONFIG_ARCH="arm64" ;;
    *)       CONFIG_ARCH="$(uname -m)" ;;
esac

# Modify config to set platform (crane append leaves these empty)
sed -i "s/\\"architecture\\":\\"\\"/\\"architecture\\":\\"${{CONFIG_ARCH}}\\"/" ${{WORK_DIR}}/${{CONFIG_FILE}}
sed -i "s/\\"os\\":\\"\\"/\\"os\\":\\"linux\\"/" ${{WORK_DIR}}/${{CONFIG_FILE}}

# Compute new hash and rename config file
NEW_HASH=$(sha256sum ${{WORK_DIR}}/${{CONFIG_FILE}} | awk '{{print $1}}')
NEW_CONFIG="sha256:${{NEW_HASH}}"
mv ${{WORK_DIR}}/${{CONFIG_FILE}} ${{WORK_DIR}}/${{NEW_CONFIG}}

# Update manifest with new config reference
sed -i "s|${{CONFIG_FILE}}|${{NEW_CONFIG}}|" ${{WORK_DIR}}/manifest.json

# Repackage tarball
pushd ${{WORK_DIR}}
tar -cf ${{VORPAL_OUTPUT}}/image.tar manifest.json ${{NEW_CONFIG}} *.tar.gz
popd

# Cleanup
rm -rf ${{WORK_DIR}}

echo "Created OCI image ${{OCI_IMAGE_NAME}}:latest\""""

        step_artifacts = [crane, rsync, self._rootfs, *self._artifacts]
        step = shell(context, step_artifacts, [], step_script, [])
        systems = [artifact_pb2.AARCH64_LINUX, artifact_pb2.X8664_LINUX]
        return (
            Artifact(self._name, [step], systems)
            .with_aliases(self._aliases)
            .build(context)
        )


# ---------------------------------------------------------------------------
# Argument
# ---------------------------------------------------------------------------


class Argument:
    """Resolves an artifact variable from the build context."""

    def __init__(self, name: str) -> None:
        self._name = name
        self._require = False

    def with_require(self) -> Argument:
        self._require = True
        return self

    def build(self, context: BuildContext) -> str | None:
        variable = context.get_variable(self._name)
        if self._require and variable is None:
            raise ValueError(f"variable '{self._name}' is required")
        return variable
