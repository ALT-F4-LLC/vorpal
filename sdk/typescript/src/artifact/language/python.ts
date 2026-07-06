import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  Artifact,
  ArtifactSource,
  DevelopmentEnvironment,
  getEnvKey,
  secretsToProto,
} from "../../artifact.js";
import { Cpython } from "../cpython.js";
import { Uv } from "../uv.js";
import { shell } from "../step.js";

/// Reproducible-build timestamp for `uv build` wheels. Wheels are zip archives, and the
/// zip format cannot represent dates before 1980 — so `SOURCE_DATE_EPOCH=0` would yield an
/// invalid wheel. This is the zip epoch (1980-01-01T00:00:00Z).
const SOURCE_DATE_EPOCH = "315532800";

/**
 * Composes the mode-specific portion of the build step script.
 *
 * App mode (`entrypoint` set) emits a relocatable launcher at `$VORPAL_OUTPUT/bin/<name>`
 * that forwards its argv (`exec … "$@"`) to the entrypoint. The launcher resolves its own
 * root at runtime via `BASH_SOURCE` (not a build-time absolute), but execs the pinned
 * interpreter by its content-addressed store path (`cpythonBin`, baked here) — that path
 * is permanent, and the per-artifact `VORPAL_ARTIFACT_*` env var is NOT set when the
 * launcher runs as a transitive dependency, so a literal env reference would break.
 *
 * HEREDOC ESCAPING DISCIPLINE: the heredoc is UNQUOTED (<< EOF), so the build-step shell
 * expands `${cpythonBin}` (baking the store path) while runtime vars use `\$` to be
 * written literally into the launcher file and expanded at launcher runtime.
 *
 * Library mode (no entrypoint) builds a wheel via `uv build` and copies the wheel/sdist,
 * `pyproject.toml`, and `uv.lock` to `$VORPAL_OUTPUT/`.
 */
export function stepBuildCommand(
  name: string,
  entrypoint: string | undefined,
  cpythonBin: string,
): string {
  if (entrypoint !== undefined) {
    return `cp -pr . "$VORPAL_OUTPUT/"

mkdir -p "$VORPAL_OUTPUT/bin"

cat > "$VORPAL_OUTPUT/bin/${name}" << EOF
#!/usr/bin/env bash
set -euo pipefail
VORPAL_PYTHON_ROOT="\\$(cd "\\$(dirname "\\\${BASH_SOURCE[0]}")/.." && pwd)"
PYTHONPATH_EXTRA="\\$VORPAL_PYTHON_ROOT"
for site in "\\$VORPAL_PYTHON_ROOT"/.venv/lib/python*/site-packages; do
    [ -d "\\$site" ] && PYTHONPATH_EXTRA="\\$site:\\$PYTHONPATH_EXTRA"
done
export PYTHONPATH="\\$PYTHONPATH_EXTRA\\\${PYTHONPATH:+:\\$PYTHONPATH}"
exec "${cpythonBin}/python3" "\\$VORPAL_PYTHON_ROOT/${entrypoint}" "\\$@"
EOF

chmod +x "$VORPAL_OUTPUT/bin/${name}"`;
  }

  return `uv build

mkdir -p "$VORPAL_OUTPUT"

cp -pr dist/. "$VORPAL_OUTPUT/"
cp pyproject.toml "$VORPAL_OUTPUT/"
cp uv.lock "$VORPAL_OUTPUT/"`;
}

export class Python {
  private _aliases: string[] = [];
  private _artifacts: string[] = [];
  private _entrypoint: string | undefined = undefined;
  private _environments: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _secrets: Map<string, string> = new Map();
  private _sourceScripts: string[] = [];
  private _systems: ArtifactSystem[];
  private _workingDir: string | undefined = undefined;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  withAliases(aliases: string[]): this {
    this._aliases = aliases;
    return this;
  }

  /** Adds artifact dependencies available during the build step. */
  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Sets the Python entrypoint (e.g. `src/main.py`).
   * When set, the builder produces a relocatable launcher (app mode).
   * When not set (default), the builder produces a wheel via `uv build` (library mode).
   */
  withEntrypoint(entrypoint: string): this {
    this._entrypoint = entrypoint;
    return this;
  }

  /** Sets environment variables for the build step. Format: "KEY=VALUE". */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  /** Sets file patterns to include in the source. */
  withIncludes(includes: string[]): this {
    this._includes = includes;
    return this;
  }

  /** Adds secrets available during the build step. */
  withSecrets(secrets: Map<string, string>): this {
    for (const [k, v] of secrets) {
      if (!this._secrets.has(k)) {
        this._secrets.set(k, v);
      }
    }
    return this;
  }

  /**
   * Adds scripts to run inside the source directory before the build.
   * Duplicates are ignored.
   */
  withSourceScripts(scripts: string[]): this {
    for (const script of scripts) {
      if (!this._sourceScripts.includes(script)) {
        this._sourceScripts.push(script);
      }
    }
    return this;
  }

  /**
   * Sets the working directory within the source. When set, the build
   * runs inside `./source/{name}/{workingDir}` instead of `./source/{name}`.
   */
  withWorkingDir(dir: string): this {
    this._workingDir = dir;
    return this;
  }

  /**
   * Builds the Python project artifact.
   *
   * `uv sync --frozen` is the hash-enforcement surface: uv verifies every package against
   * the per-package SHA-256 in the committed `uv.lock` and fails closed on a content-hash
   * mismatch (there is no `uv sync --require-hashes` flag — that is uv's pip-interface
   * flag). `UV_PYTHON_DOWNLOADS=never` + `UV_PYTHON` pinned to the Vorpal interpreter
   * guarantee uv never fetches an interpreter at build time.
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
    // Setup toolchain artifacts
    const cpythonDigest = await new Cpython().build(context);
    const cpythonBin = `${getEnvKey(cpythonDigest)}/bin`;

    const uvDigest = await new Uv().build(context);
    const uvBin = `${getEnvKey(uvDigest)}/bin`;

    // Setup source
    const sourcePath = ".";
    const sourceBuilder = new ArtifactSource(this._name, sourcePath);

    if (this._includes.length > 0) {
      sourceBuilder.withIncludes(this._includes);
    }

    const source = sourceBuilder.build();

    // Setup step source directory
    let stepSourceDir = `${sourcePath}/source/${source.name}`;

    if (this._workingDir !== undefined) {
      stepSourceDir = `${stepSourceDir}/${this._workingDir}`;
    }

    // TRUST: `name`, `entrypoint`, and `workingDir` are interpolated unescaped into the
    // build shell — CONFIG-AUTHOR-CONTROLLED (workspace trust, same as withSourceScripts).
    const stepBuildCmd = stepBuildCommand(
      this._name,
      this._entrypoint,
      cpythonBin,
    );

    // Build step script — matches Rust formatdoc! output
    const stepScript = `pushd ${stepSourceDir}

${this._sourceScripts.join("\n")}

uv sync --frozen --no-dev --no-editable

${stepBuildCmd}`;

    const stepEnvironments = [
      `PATH=${uvBin}:${cpythonBin}`,
      `UV_PYTHON=${cpythonBin}/python3`,
      "UV_PYTHON_DOWNLOADS=never",
      "UV_LINK_MODE=copy",
      "UV_CACHE_DIR=$VORPAL_WORKSPACE/uv/cache",
      `SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH}`,
      ...this._environments,
    ];

    const stepArtifacts = [cpythonDigest, uvDigest, ...this._artifacts];

    const step = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      stepScript,
      secretsToProto(this._secrets),
    );

    return new Artifact(this._name, [step], this._systems)
      .withAliases(this._aliases)
      .withSources([source])
      .build(context);
  }
}

// ---------------------------------------------------------------------------
// Python Development Environment
// ---------------------------------------------------------------------------

/**
 * Builder for Python development environment artifacts.
 *
 * Wraps {@link DevelopmentEnvironment} to provide a Python-specific
 * development environment with CPython and uv pre-configured.
 * Pins UV_PYTHON to the Vorpal-managed interpreter and sets
 * UV_PYTHON_DOWNLOADS=never to prevent uv from fetching interpreters
 * at dev-shell entry.
 *
 * Usage:
 * ```typescript
 * const digest = await new PythonDevelopmentEnvironment("example-shell", SYSTEMS)
 *   .build(context);
 * ```
 */
export class PythonDevelopmentEnvironment {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _secrets: Map<string, string> = new Map();
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Adds extra artifact dependencies beyond the default CPython + uv tooling.
   * These are appended to the default artifacts, not replacing them.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts.push(...artifacts);
    return this;
  }

  /**
   * Adds extra environment variables beyond the default UV_PYTHON/UV_PYTHON_DOWNLOADS.
   * Format: "KEY=VALUE".
   */
  withEnvironments(environments: string[]): this {
    this._environments.push(...environments);
    return this;
  }

  /** Adds secrets available during the environment build step. */
  withSecrets(secrets: Map<string, string>): this {
    for (const [k, v] of secrets) {
      if (!this._secrets.has(k)) {
        this._secrets.set(k, v);
      }
    }
    return this;
  }

  /**
   * Builds the Python development environment artifact.
   *
   * Default artifacts fetched:
   * - cpython (CPython interpreter)
   * - uv (Python package manager)
   *
   * Default environment variables:
   * - UV_PYTHON: pinned to the Vorpal-managed CPython interpreter
   * - UV_PYTHON_DOWNLOADS=never: prevents uv from fetching interpreters at dev-shell entry
   */
  async build(context: ConfigContext): Promise<string> {
    const cpython = await new Cpython().build(context);
    const cpythonBin = `${getEnvKey(cpython)}/bin`;

    const uv = await new Uv().build(context);

    const artifacts: string[] = [cpython, uv, ...this._artifacts];

    // Pin the dev-shell interpreter and suppress uv's auto-download so the
    // shell always uses the Vorpal-managed CPython (Go/Rust env-var pattern).
    const environments: string[] = [
      `UV_PYTHON=${cpythonBin}/python3`,
      "UV_PYTHON_DOWNLOADS=never",
      ...this._environments,
    ];

    let devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.size > 0) {
      devenv = devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}
