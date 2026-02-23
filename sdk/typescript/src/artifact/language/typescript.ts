import type {
  ArtifactSource as ArtifactSourceMsg,
  ArtifactStepSecret,
} from "../../api/artifact/artifact.js";
import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  Artifact,
  ArtifactSource,
  DevelopmentEnvironment,
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
 * const digest = await new TypeScript("my-app", SYSTEMS)
 *   .withEntrypoint("src/index.ts")
 *   .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
 *   .build(context);
 * ```
 */
export class TypeScript {
  private _artifacts: string[] = [];
  private _bun: string | undefined = undefined;
  private _entrypoint: string | undefined = undefined;
  private _environments: string[] = [];
  private _excludes: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _secrets: ArtifactStepSecret[] = [];
  private _source: ArtifactSourceMsg | undefined = undefined;
  private _nodeModules: Map<string, string> = new Map();
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
   * Maps an npm package name to a Vorpal store artifact.
   * At build time, a symlink is created in node_modules/ pointing
   * to the artifact's output directory.
   *
   * @param packageName - npm package name (e.g., "@vorpal/sdk")
   * @param digest - Artifact digest for the package
   */
  withNodeModule(packageName: string, digest: string): this {
    this._nodeModules.set(packageName, digest);
    return this;
  }

  /**
   * Maps multiple npm packages to Vorpal store artifacts.
   *
   * @param modules - Array of [packageName, digest] tuples
   */
  withNodeModules(modules: Array<[string, string]>): this {
    for (const [name, digest] of modules) {
      this._nodeModules.set(name, digest);
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

    // Sort node modules alphabetically by package name for deterministic output
    const sortedNodeModules = [...this._nodeModules.entries()].sort((a, b) =>
      a[0].localeCompare(b[0])
    );

    // Build source
    const sourcePath = ".";
    let source: ArtifactSourceMsg;

    if (this._source !== undefined) {
      source = this._source;
    } else {
      const sourceBuilder = new ArtifactSource(this._name, sourcePath);

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

    // Build node modules script
    let nodeModulesScript = "";
    if (sortedNodeModules.length > 0) {
      const lines: string[] = ["mkdir -p node_modules"];
      for (const [packageName, digest] of sortedNodeModules) {
        if (packageName.includes("/")) {
          const scope = packageName.split("/")[0];
          lines.push(`mkdir -p node_modules/${scope}`);
        }
        lines.push(
          `ln -sf ${getEnvKey(digest)} node_modules/${packageName}`
        );
      }
      nodeModulesScript = `\n${lines.join("\n")}`;
    }

    // Build step script -- mirrors the Rust-side TypeScript language builder
    const sourceDir = `./source/${source.name}`;
    const stepScript = `pushd "${sourceDir}"

mkdir -p "$VORPAL_OUTPUT/bin"
${sourceScriptsStr}${nodeModulesScript}
${bunBin}/bun install --frozen-lockfile
${bunBin}/bun build --compile "${entrypoint}" --outfile "$VORPAL_OUTPUT/bin/${this._name}"`;

    // Build environment variables
    const stepEnvironments = [`PATH=${bunBin}`, ...this._environments];

    // Build artifact dependencies
    const stepArtifacts = [bunDigest, ...this._artifacts];

    // Add node module artifact digests
    for (const [, digest] of sortedNodeModules) {
      stepArtifacts.push(digest);
    }

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

    const environments: string[] = [...this._environments];

    let devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.length > 0) {
      devenv = devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}

// ---------------------------------------------------------------------------
// TypeScript Library Builder
// ---------------------------------------------------------------------------

/**
 * Builder for TypeScript library/package artifacts.
 *
 * Unlike {@link TypeScript} which produces a standalone compiled binary via
 * `bun build --compile`, this builder produces an npm-style package artifact
 * containing `dist/`, `package.json`, and `node_modules/`. The output is
 * suitable for consumption by other TypeScript artifacts via
 * {@link TypeScript.withNodeModule} or
 * {@link TypeScriptDevelopmentEnvironment}.
 *
 * The builder:
 * 1. Accepts project source path and build configuration
 * 2. Uses Bun to install dependencies
 * 3. Runs a configurable build command (default: `bun run build`)
 * 4. Copies `package.json`, `dist/`, and `node_modules/` to `$VORPAL_OUTPUT`
 *
 * By default, the builder fetches the Bun toolchain from the registry
 * using the alias "bun:1.2.0". Use `.withBun(digest)` to override.
 *
 * Usage:
 * ```typescript
 * const digest = await new TypeScriptLibrary("my-lib", SYSTEMS)
 *   .withIncludes(["src", "package.json", "tsconfig.json", "bun.lockb"])
 *   .build(context);
 * ```
 */
export class TypeScriptLibrary {
  private _artifacts: string[] = [];
  private _bun: string | undefined = undefined;
  private _buildCommand: string = "bun run build";
  private _environments: string[] = [];
  private _excludes: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _nodeModules: Map<string, string> = new Map();
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
   * Sets the build command used to compile the library.
   * Defaults to "bun run build".
   *
   * @param command - Build command to run (e.g., "bun run build", "npx tsc")
   */
  withBuildCommand(command: string): this {
    this._buildCommand = command;
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
   * Maps an npm package name to a Vorpal store artifact.
   * At build time, a symlink is created in node_modules/ pointing
   * to the artifact's output directory.
   *
   * @param packageName - npm package name (e.g., "@vorpal/sdk")
   * @param digest - Artifact digest for the package
   */
  withNodeModule(packageName: string, digest: string): this {
    this._nodeModules.set(packageName, digest);
    return this;
  }

  /**
   * Maps multiple npm packages to Vorpal store artifacts.
   *
   * @param modules - Array of [packageName, digest] tuples
   */
  withNodeModules(modules: Array<[string, string]>): this {
    for (const [name, digest] of modules) {
      this._nodeModules.set(name, digest);
    }
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
   * Builds the TypeScript library artifact.
   *
   * Produces an npm-style package at $VORPAL_OUTPUT containing
   * `package.json`, `dist/`, and `node_modules/`. The build process:
   * 1. Changes to the source directory
   * 2. Runs any source scripts
   * 3. Creates symlinks for store-resolved node modules
   * 4. Installs dependencies via `bun install --frozen-lockfile`
   * 5. Runs the build command (default: `bun run build`)
   * 6. Copies package.json, dist/, and node_modules/ to $VORPAL_OUTPUT
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
    // Sort secrets for deterministic output
    this._secrets.sort((a, b) => a.name.localeCompare(b.name));

    // Sort node modules alphabetically by package name for deterministic output
    const sortedNodeModules = [...this._nodeModules.entries()].sort((a, b) =>
      a[0].localeCompare(b[0])
    );

    // Build source
    const sourcePath = ".";
    let source: ArtifactSourceMsg;

    if (this._source !== undefined) {
      source = this._source;
    } else {
      const sourceBuilder = new ArtifactSource(this._name, sourcePath);

      if (this._includes.length > 0) {
        sourceBuilder.withIncludes(this._includes);
      }

      if (this._excludes.length > 0) {
        sourceBuilder.withExcludes(this._excludes);
      }

      source = sourceBuilder.build();
    }

    // Resolve Bun artifact -- use provided digest or fetch from registry
    const bunDigest =
      this._bun ?? (await context.fetchArtifactAlias(DEFAULT_BUN_ALIAS));

    const bunBin = `${getEnvKey(bunDigest)}/bin`;

    // Build source scripts section
    const sourceScriptsStr =
      this._sourceScripts.length > 0
        ? `\n${this._sourceScripts.join("\n")}\n`
        : "";

    // Build node modules script
    let nodeModulesScript = "";
    if (sortedNodeModules.length > 0) {
      const lines: string[] = ["mkdir -p node_modules"];
      for (const [packageName, digest] of sortedNodeModules) {
        if (packageName.includes("/")) {
          const scope = packageName.split("/")[0];
          lines.push(`mkdir -p node_modules/${scope}`);
        }
        lines.push(
          `ln -sf ${getEnvKey(digest)} node_modules/${packageName}`
        );
      }
      nodeModulesScript = `\n${lines.join("\n")}`;
    }

    // Build step script -- library mode: produces dist/ + package.json + node_modules/
    const sourceDir = `./source/${source.name}`;
    const stepScript = `pushd "${sourceDir}"
${sourceScriptsStr}${nodeModulesScript}
${bunBin}/bun install --frozen-lockfile
${bunBin}/${this._buildCommand}

mkdir -p "$VORPAL_OUTPUT"
cp package.json "$VORPAL_OUTPUT/"
cp -r dist "$VORPAL_OUTPUT/"
cp -r node_modules "$VORPAL_OUTPUT/"`;

    // Build environment variables
    const stepEnvironments = [`PATH=${bunBin}`, ...this._environments];

    // Build artifact dependencies
    const stepArtifacts = [bunDigest, ...this._artifacts];

    // Add node module artifact digests
    for (const [, digest] of sortedNodeModules) {
      stepArtifacts.push(digest);
    }

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
      .withSources([source])
      .build(context);
  }
}
