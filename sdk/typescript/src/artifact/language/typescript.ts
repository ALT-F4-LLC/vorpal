import type {
  ArtifactSource as ArtifactSourceMsg,
  ArtifactStepSecret,
} from "../../api/artifact/artifact.js";
import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  ArtifactBuilder,
  ArtifactSourceBuilder,
  getEnvKey,
} from "../../artifact.js";
import { shell } from "../step.js";

/**
 * Default Bun artifact alias used when no explicit Bun digest is provided.
 * Matches the alias registered by the Rust-side Bun artifact builder.
 */
const DEFAULT_BUN_ALIAS = "bun:1.2.0";

/**
 * Builder for TypeScript/Node.js project artifacts.
 *
 * Analogous to the Go SDK's `language.NewGo()` and the Rust SDK's
 * `language::Rust` -- this builder bundles and compiles TypeScript/Node.js
 * projects as standalone Vorpal artifacts using Bun.
 *
 * The builder:
 * 1. Accepts project source path, entry point, and output binary name
 * 2. Uses Bun to install dependencies and compile to a standalone binary
 * 3. Supports customizable build commands for different project types
 * 4. Produces output at $VORPAL_OUTPUT with the compiled project
 *
 * By default, the builder fetches the Bun toolchain from the registry
 * using the alias "bun:1.2.0". Use `.withBun(digest)` to override.
 *
 * Usage:
 * ```typescript
 * const digest = await new TypeScriptBuilder("my-app", SYSTEMS)
 *   .withEntrypoint("src/index.ts")
 *   .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
 *   .build(context);
 * ```
 */
export class TypeScriptBuilder {
  private _artifacts: string[] = [];
  private _bun: string | undefined = undefined;
  private _entrypoint: string | undefined = undefined;
  private _environments: string[] = [];
  private _excludes: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _source: ArtifactSourceMsg | undefined = undefined;
  private _sourceScripts: string[] = [];
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Adds artifact dependencies that will be available during the build step.
   * These artifacts' bin directories are added to PATH.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Sets the Bun artifact digest to use for the build.
   * If not set, the builder fetches the default Bun artifact from the
   * registry using the alias "bun:1.2.0".
   */
  withBun(bun: string): this {
    this._bun = bun;
    return this;
  }

  /**
   * Sets the entrypoint TypeScript file for the project.
   * Defaults to "src/{name}.ts" if not set.
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
   * Sets file patterns to exclude from the source.
   * Only used when no explicit source is provided via withSource.
   */
  withExcludes(excludes: string[]): this {
    this._excludes = excludes;
    return this;
  }

  /**
   * Sets file patterns to include in the source.
   * Only used when no explicit source is provided via withSource.
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
   * Sets an explicit ArtifactSource for the project.
   * If not set, one is constructed from the name with any includes/excludes.
   */
  withSource(source: ArtifactSourceMsg): this {
    this._source = source;
    return this;
  }

  /**
   * Adds a script to run inside the source directory before the build.
   * Multiple scripts are joined with newlines and run in order.
   */
  withSourceScript(script: string): this {
    if (!this._sourceScripts.includes(script)) {
      this._sourceScripts.push(script);
    }
    return this;
  }

  /**
   * Builds the TypeScript project artifact.
   *
   * Produces a standalone compiled binary at $VORPAL_OUTPUT/bin/{name}
   * using Bun's --compile flag. The build process:
   * 1. Changes to the source directory
   * 2. Runs any source scripts
   * 3. Installs dependencies via `bun install --frozen-lockfile`
   * 4. Compiles to standalone binary via `bun build --compile`
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
    // Sort secrets for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    // Build source
    const sourcePath = ".";
    let source: ArtifactSourceMsg;

    if (this._source !== undefined) {
      source = this._source;
    } else {
      const sourceBuilder = new ArtifactSourceBuilder(this._name, sourcePath);

      if (this._includes.length > 0) {
        sourceBuilder.withIncludes(this._includes);
      }

      if (this._excludes.length > 0) {
        sourceBuilder.withExcludes(this._excludes);
      }

      source = sourceBuilder.build();
    }

    // Resolve entrypoint
    const entrypoint = this._entrypoint ?? `src/${this._name}.ts`;

    // Resolve Bun artifact -- use provided digest or fetch from registry
    const bunDigest =
      this._bun ?? (await context.fetchArtifactAlias(DEFAULT_BUN_ALIAS));

    const bunBin = `${getEnvKey(bunDigest)}/bin`;

    // Build source scripts section
    const sourceScriptsStr =
      this._sourceScripts.length > 0
        ? `\n${this._sourceScripts.join("\n")}\n`
        : "";

    // Build step script -- mirrors the Rust-side TypeScript language builder
    const sourceDir = `./source/${source.name}`;
    const stepScript = `pushd "${sourceDir}"

mkdir -p "$VORPAL_OUTPUT/bin"
${sourceScriptsStr}
${bunBin}/bun install --frozen-lockfile
${bunBin}/bun build --compile "${entrypoint}" --outfile "$VORPAL_OUTPUT/bin/${this._name}"`;

    // Build environment variables
    const stepEnvironments = [`PATH=${bunBin}`, ...this._environments];

    // Build artifact dependencies
    const stepArtifacts = [bunDigest, ...this._artifacts];

    // Create step
    const step = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      stepScript,
      this._secrets,
    );

    // Create and return artifact
    return new ArtifactBuilder(this._name, [step], this._systems)
      .withSources([source])
      .build(context);
  }
}
