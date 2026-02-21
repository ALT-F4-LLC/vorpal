import type {
  Artifact as ArtifactMsg,
  ArtifactSource as ArtifactSourceMsg,
  ArtifactStep as ArtifactStepMsg,
  ArtifactStepSecret,
} from "./api/artifact/artifact.js";
import { ArtifactSystem } from "./api/artifact/artifact.js";
import { shell } from "./artifact/step.js";
import type { ConfigContext } from "./context.js";

/**
 * Returns the environment variable key for an artifact digest.
 * Matches Rust get_env_key() and Go GetEnvKey().
 */
export function getEnvKey(digest: string): string {
  return `$VORPAL_ARTIFACT_${digest}`;
}

// ---------------------------------------------------------------------------
// ArtifactSourceBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for ArtifactSource messages.
 * Matches Rust sdk/rust/src/artifact.rs ArtifactSource impl.
 */
export class ArtifactSourceBuilder {
  private _digest: string | undefined = undefined;
  private _excludes: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _path: string;

  constructor(name: string, path: string) {
    this._name = name;
    this._path = path;
  }

  withDigest(digest: string): this {
    this._digest = digest;
    return this;
  }

  withExcludes(excludes: string[]): this {
    this._excludes = excludes;
    return this;
  }

  withIncludes(includes: string[]): this {
    this._includes = includes;
    return this;
  }

  build(): ArtifactSourceMsg {
    return {
      digest: this._digest,
      excludes: this._excludes,
      includes: this._includes,
      name: this._name,
      path: this._path,
    };
  }
}

// ---------------------------------------------------------------------------
// ArtifactStepBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for ArtifactStep messages.
 * Matches Rust sdk/rust/src/artifact.rs ArtifactStep impl.
 */
export class ArtifactStepBuilder {
  private _arguments: string[] = [];
  private _artifacts: string[] = [];
  private _entrypoint: string;
  private _environments: string[] = [];
  private _secrets: ArtifactStepSecret[] = [];
  private _script: string | undefined = undefined;

  constructor(entrypoint: string) {
    this._entrypoint = entrypoint;
  }

  withArguments(args: string[]): this {
    this._arguments = args;
    return this;
  }

  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  withSecrets(secrets: ArtifactStepSecret[]): this {
    for (const secret of secrets) {
      if (!this._secrets.some((s) => s.name === secret.name)) {
        this._secrets.push(secret);
      }
    }
    return this;
  }

  withScript(script: string): this {
    this._script = script;
    return this;
  }

  build(): ArtifactStepMsg {
    return {
      entrypoint: this._entrypoint,
      script: this._script,
      secrets: this._secrets,
      arguments: this._arguments,
      artifacts: this._artifacts,
      environments: this._environments,
    };
  }
}

// ---------------------------------------------------------------------------
// ArtifactBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for Artifact messages.
 * Matches Rust sdk/rust/src/artifact.rs Artifact impl (lines 211-256).
 */
export class ArtifactBuilder {
  private _aliases: string[] = [];
  private _name: string;
  private _sources: ArtifactSourceMsg[] = [];
  private _steps: ArtifactStepMsg[];
  private _systems: ArtifactSystem[];

  constructor(
    name: string,
    steps: ArtifactStepMsg[],
    systems: ArtifactSystem[],
  ) {
    this._name = name;
    this._steps = steps;
    this._systems = systems;
  }

  withAliases(aliases: string[]): this {
    for (const alias of aliases) {
      if (!this._aliases.includes(alias)) {
        this._aliases.push(alias);
      }
    }
    return this;
  }

  withSources(sources: ArtifactSourceMsg[]): this {
    for (const source of sources) {
      if (!this._sources.some((s) => s.name === source.name)) {
        this._sources.push(source);
      }
    }
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const artifact: ArtifactMsg = {
      target: context.getSystem(),
      sources: this._sources,
      steps: this._steps,
      systems: this._systems,
      aliases: this._aliases,
      name: this._name,
    };

    return context.addArtifact(artifact);
  }
}

// ---------------------------------------------------------------------------
// JobBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for Job artifacts (simple script execution).
 * Matches Rust sdk/rust/src/artifact.rs Job impl (lines 259-297).
 *
 * CRITICAL: Secrets are sorted by name before building (Rust line 290).
 */
export class JobBuilder {
  private _artifacts: string[] = [];
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _script: string;
  private _systems: ArtifactSystem[];

  constructor(name: string, script: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._script = script;
    this._systems = systems;
  }

  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some((s) => s.name === name)) {
        this._secrets.push({ name, value });
      }
    }
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    // Sort for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    const step = await shell(
      context,
      this._artifacts,
      [],
      this._script,
      this._secrets,
    );

    return new ArtifactBuilder(this._name, [step], this._systems).build(
      context,
    );
  }
}

// ---------------------------------------------------------------------------
// ProcessBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for Process artifacts.
 * Matches Rust sdk/rust/src/artifact.rs Process impl (lines 432-553).
 *
 * CRITICAL: Shell script template must be character-for-character identical.
 * Secrets sorted by name (Rust line 477).
 */
export class ProcessBuilder {
  private _arguments: string[] = [];
  private _artifacts: string[] = [];
  private _entrypoint: string;
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _systems: ArtifactSystem[];

  constructor(
    name: string,
    entrypoint: string,
    systems: ArtifactSystem[],
  ) {
    this._name = name;
    this._entrypoint = entrypoint;
    this._systems = systems;
  }

  withArguments(args: string[]): this {
    this._arguments = args;
    return this;
  }

  withArtifacts(artifacts: string[]): this {
    for (const artifact of artifacts) {
      if (!this._artifacts.includes(artifact)) {
        this._artifacts.push(artifact);
      }
    }
    return this;
  }

  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some((s) => s.name === name)) {
        this._secrets.push({ name, value });
      }
    }
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    // Sort for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    const argumentsStr = this._arguments.join(" ");

    const artifactsStr = this._artifacts
      .map((v) => `$VORPAL_ARTIFACT_${v}/bin`)
      .join(":");

    // Script template matches Rust formatdoc! in Process::build()
    const script = `mkdir -pv $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/${this._name}-logs << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/logs.txt ]; then
    tail -f $VORPAL_OUTPUT/logs.txt
else
    echo "No logs found"
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/${this._name}-logs

cat > $VORPAL_OUTPUT/bin/${this._name}-stop << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/pid ]; then
    kill $(cat $VORPAL_OUTPUT/pid)
    rm -rf $VORPAL_OUTPUT/pid
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/${this._name}-stop

cat > $VORPAL_OUTPUT/bin/${this._name}-start << "EOF"
#!/bin/bash
set -euo pipefail

export PATH=${artifactsStr}:$PATH

$VORPAL_OUTPUT/bin/${this._name}-stop

echo "Process: ${this._entrypoint} ${argumentsStr}"

nohup ${this._entrypoint} ${argumentsStr} > $VORPAL_OUTPUT/logs.txt 2>&1 &

PROCESS_PID=$!

echo "Process ID: $PROCESS_PID"

echo $PROCESS_PID > $VORPAL_OUTPUT/pid

echo "Process commands:"
echo "- ${this._name}-logs (tail logs)"
echo "- ${this._name}-stop (stop process)"
echo "- ${this._name}-start (start process)"
EOF

chmod +x $VORPAL_OUTPUT/bin/${this._name}-start`;

    const step = await shell(
      context,
      this._artifacts,
      [],
      script,
      this._secrets,
    );

    return new ArtifactBuilder(this._name, [step], this._systems).build(
      context,
    );
  }
}

// ---------------------------------------------------------------------------
// ProjectEnvironmentBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for ProjectEnvironment artifacts.
 * Matches Rust sdk/rust/src/artifact.rs ProjectEnvironment impl (lines 300-429).
 *
 * CRITICAL: Shell script template must match exactly. Secrets sorted by name.
 */
export class ProjectEnvironmentBuilder {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some((s) => s.name === name)) {
        this._secrets.push({ name, value });
      }
    }
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    // Sort for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    const envsBackup = [
      'export VORPAL_SHELL_BACKUP_PATH="$PATH"',
      'export VORPAL_SHELL_BACKUP_PS1="$PS1"',
      'export VORPAL_SHELL_BACKUP_VORPAL_SHELL="$VORPAL_SHELL"',
    ];

    const envsExport = [
      `export PS1="(${this._name}) $PS1"`,
      'export VORPAL_SHELL="1"',
    ];

    const envsRestore = [
      'export PATH="$VORPAL_SHELL_BACKUP_PATH"',
      'export PS1="$VORPAL_SHELL_BACKUP_PS1"',
      'export VORPAL_SHELL="$VORPAL_SHELL_BACKUP_VORPAL_SHELL"',
    ];

    const envsUnset = [
      "unset VORPAL_SHELL_BACKUP_PATH",
      "unset VORPAL_SHELL_BACKUP_PS1",
      "unset VORPAL_SHELL_BACKUP_VORPAL_SHELL",
    ];

    for (const env of this._environments) {
      const key = env.split("=")[0];

      if (key === "PATH") {
        continue;
      }

      envsBackup.push(
        `export VORPAL_SHELL_BACKUP_${key}="\$${key}"`,
      );
      envsExport.push(`export ${env}`);
      envsRestore.push(
        `export ${key}="\$VORPAL_SHELL_BACKUP_${key}"`,
      );
      envsUnset.push(`unset VORPAL_SHELL_BACKUP_${key}`);
    }

    // Setup path
    const stepPathArtifacts = this._artifacts
      .map((artifact) => `${getEnvKey(artifact)}/bin`)
      .join(":");

    let stepPath = stepPathArtifacts;

    const pathEnv = this._environments.find((x) => x.startsWith("PATH="));
    if (pathEnv) {
      const pathValue = pathEnv.split("=").slice(1).join("=");
      if (pathValue) {
        stepPath = `${pathValue}:${stepPath}`;
      }
    }

    envsExport.push(`export PATH=${stepPath}:$PATH`);

    // Setup script - matches Rust formatdoc!
    const stepScript = `mkdir -pv $VORPAL_WORKSPACE/bin

cat > bin/activate << "EOF"
#!/bin/bash

${envsBackup.join("\n")}
${envsExport.join("\n")}

deactivate(){
${envsRestore.join("\n")}
${envsUnset.join("\n")}
}

exec "$@"
EOF

chmod +x $VORPAL_WORKSPACE/bin/activate

mkdir -pv $VORPAL_OUTPUT/bin

cp -prv bin "$VORPAL_OUTPUT"`;

    const steps = [
      await shell(
        context,
        this._artifacts,
        [],
        stepScript,
        this._secrets,
      ),
    ];

    return new ArtifactBuilder(this._name, steps, this._systems).build(
      context,
    );
  }
}

// ---------------------------------------------------------------------------
// UserEnvironmentBuilder
// ---------------------------------------------------------------------------

/**
 * Builder for UserEnvironment artifacts.
 * Matches Rust sdk/rust/src/artifact.rs UserEnvironment impl (lines 556-682).
 *
 * CRITICAL: Symlinks MUST be sorted by source path (Rust line 586).
 */
export class UserEnvironmentBuilder {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _symlinks: Array<[string, string]> = [];
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  withSymlinks(symlinks: Array<[string, string]>): this {
    for (const [source, target] of symlinks) {
      this._symlinks.push([source, target]);
    }
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    // Sort for deterministic output -- sorted by source path (index 0)
    this._symlinks.sort((a, b) => a[0].localeCompare(b[0]));

    // Setup path
    const stepPathArtifacts = this._artifacts
      .map((artifact) => `${getEnvKey(artifact)}/bin`)
      .join(":");

    let stepPath = stepPathArtifacts;

    const pathEnv = this._environments.find((x) => x.startsWith("PATH="));
    if (pathEnv) {
      const pathValue = pathEnv.split("=").slice(1).join("=");
      if (pathValue) {
        stepPath = `${pathValue}:${stepPath}`;
      }
    }

    // Setup environments for script (filter PATH)
    const stepEnvironments = this._environments
      .filter((e) => !e.startsWith("PATH="))
      .map((e) => `export ${e}`)
      .join("\n");

    const symlinksDeactivate = this._symlinks
      .map(([_, target]) => `rm -fv ${target}`)
      .join("\n");

    const symlinksCheck = this._symlinks
      .map(
        ([_, target]) =>
          `if [ -f ${target} ]; then echo "ERROR: Symlink target exists -> ${target}" && exit 1; fi`,
      )
      .join("\n");

    const symlinksActivate = this._symlinks
      .map(([source, target]) => `ln -sv ${source} ${target}`)
      .join("\n");

    // Script template matches Rust formatdoc! in UserEnvironment::build()
    const stepScript = `mkdir -pv $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/vorpal-activate-shell << "EOF"
${stepEnvironments}
export PATH="$VORPAL_OUTPUT/bin:${stepPath}:$PATH"
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
${symlinksDeactivate}
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-activate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
${symlinksCheck}
${symlinksActivate}
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

ln -sfv $VORPAL_OUTPUT/bin/vorpal-activate-shell $HOME/.vorpal/bin/vorpal-activate-shell
ln -sfv $VORPAL_OUTPUT/bin/vorpal-activate-symlinks $HOME/.vorpal/bin/vorpal-activate-symlinks
ln -sfv $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks $HOME/.vorpal/bin/vorpal-deactivate-symlinks
EOF


chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-shell
chmod +x $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate`;

    const steps = [
      await shell(context, this._artifacts, [], stepScript, []),
    ];

    return new ArtifactBuilder(this._name, steps, this._systems).build(
      context,
    );
  }
}

// ---------------------------------------------------------------------------
// Argument
// ---------------------------------------------------------------------------

/**
 * Argument builder for artifact variables.
 * Matches Rust sdk/rust/src/artifact.rs Argument impl.
 */
export class Argument {
  private _name: string;
  private _require: boolean = false;

  constructor(name: string) {
    this._name = name;
  }

  withRequire(): this {
    this._require = true;
    return this;
  }

  build(context: ConfigContext): string | undefined {
    const variable = context.getVariable(this._name);

    if (this._require && variable === undefined) {
      throw new Error(`variable '${this._name}' is required`);
    }

    return variable;
  }
}
