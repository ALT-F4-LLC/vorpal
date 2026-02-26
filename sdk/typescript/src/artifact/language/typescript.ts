import type {
  ArtifactStepSecret,
  ArtifactSystem,
} from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  Artifact,
  ArtifactSource,
  DevelopmentEnvironment,
  getEnvKey,
} from "../../artifact.js";
import { shell } from "../step.js";

const DEFAULT_BUN_ALIAS = "bun:1.2.0";

export class TypeScript {
  private _aliases: string[] = [];
  private _artifacts: string[] = [];
  private _entrypoint: string | undefined = undefined;
  private _environments: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _sourceScripts: string[] = [];
  private _systems: ArtifactSystem[];
  private _workingDir: string | undefined = undefined;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
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

  /**
   * Adds artifact dependencies that will be available during the build step.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Sets the entrypoint TypeScript file for the project.
   * When set, the builder produces a compiled binary (binary mode).
   * When not set (default), the builder produces a library package (library mode).
   */
  withEntrypoint(entrypoint: string): this {
    this._entrypoint = entrypoint;
    return this;
  }

  /**
   * Sets environment variables for the build step.
   * Format: "KEY=VALUE"
   */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  /**
   * Sets file patterns to include in the source.
   */
  withIncludes(includes: string[]): this {
    this._includes = includes;
    return this;
  }

  /**
   * Adds secrets available during the build step.
   * Secrets are deduplicated by name.
   */
  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some((s) => s.name === name)) {
        this._secrets.push({ name, value });
      }
    }
    return this;
  }

  /**
   * Adds scripts to run inside the source directory before the build.
   * Multiple scripts are joined with newlines and run in order.
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
   * Builds the TypeScript project artifact.
   *
   * When entrypoint is set (binary mode):
   * - Compiles to standalone binary via `bun build --compile`
   * - Output at `$VORPAL_OUTPUT/bin/{name}`
   *
   * When entrypoint is undefined (library mode):
   * - Builds via `bun x tsc --project tsconfig.json --outDir dist`
   * - Copies `package.json`, `dist/`, `node_modules/` to `$VORPAL_OUTPUT/`
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
    // Setup artifacts -- resolve Bun
    const bunDigest = await context.fetchArtifactAlias(DEFAULT_BUN_ALIAS);
    const bunBin = `${getEnvKey(bunDigest)}/bin`;

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

    // Setup build command -- dual mode based on entrypoint
    let stepBuildCommand: string;

    if (this._entrypoint !== undefined) {
      // Binary mode
      stepBuildCommand = `mkdir -p $VORPAL_OUTPUT/bin

${bunBin}/bun build --compile ${this._entrypoint} --outfile ${this._name}

cp ${this._name} $VORPAL_OUTPUT/bin/${this._name}`;
    } else {
      // Library mode
      stepBuildCommand = `mkdir -p $VORPAL_OUTPUT

${bunBin}/bun x tsc --project tsconfig.json --outDir dist

cp package.json $VORPAL_OUTPUT/
cp -r dist $VORPAL_OUTPUT/
cp -r node_modules $VORPAL_OUTPUT/`;
    }

    // Build step script -- matches Rust formatdoc! output
    const stepScript = `pushd ${stepSourceDir}

${this._sourceScripts.join("\n")}

${bunBin}/bun install --frozen-lockfile

${stepBuildCommand}`;

    // Build environment variables
    const stepEnvironments = [`PATH=${bunBin}`, ...this._environments];

    // Build artifact dependencies
    const stepArtifacts = [bunDigest, ...this._artifacts];

    // Sort secrets for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    // Create step
    const step = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      stepScript,
      this._secrets,
    );

    // Create and return artifact
    return new Artifact(this._name, [step], this._systems)
      .withAliases(this._aliases)
      .withSources([source])
      .build(context);
  }
}

// ---------------------------------------------------------------------------
// TypeScript Development Environment
// ---------------------------------------------------------------------------

/**
 * Builder for TypeScript development environment artifacts.
 *
 * Wraps {@link DevelopmentEnvironment} to provide a TypeScript-specific
 * development environment with Bun pre-configured. This is the simplest
 * of the language-specific development environment builders -- it includes
 * only the Bun runtime as a default tool and requires no special
 * environment variables.
 *
 * Usage:
 * ```typescript
 * const digest = await new TypeScriptDevelopmentEnvironment("example-shell", SYSTEMS)
 *   .build(context);
 * ```
 */
export class TypeScriptDevelopmentEnvironment {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _secrets: Array<[string, string]> = [];
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Adds extra artifact dependencies beyond the default Bun tooling.
   * These are appended to the default artifacts, not replacing them.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts.push(...artifacts);
    return this;
  }

  /**
   * Adds extra environment variables beyond what DevelopmentEnvironment provides.
   * Format: "KEY=VALUE".
   */
  withEnvironments(environments: string[]): this {
    this._environments.push(...environments);
    return this;
  }

  /** Adds secrets available during the environment build step. Duplicates (by name) are ignored. */
  withSecrets(secrets: Array<[string, string]>): this {
    for (const [name, value] of secrets) {
      if (!this._secrets.some(([n]) => n === name)) {
        this._secrets.push([name, value]);
      }
    }
    return this;
  }

  /**
   * Builds the TypeScript development environment artifact.
   *
   * Default artifacts fetched:
   * - bun (Bun runtime)
   *
   * No default environment variables beyond what DevelopmentEnvironment provides.
   * Bun does not require special env vars like Go or Rust do.
   */
  async build(context: ConfigContext): Promise<string> {
    const bun = await context.fetchArtifactAlias(DEFAULT_BUN_ALIAS);

    const artifacts: string[] = [bun, ...this._artifacts];

    let devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(this._environments);

    if (this._secrets.length > 0) {
      devenv = devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}

