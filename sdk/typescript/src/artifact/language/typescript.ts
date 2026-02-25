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

/**
 * Default Bun artifact alias used when no explicit Bun digest is provided.
 * Matches the alias registered by the Rust-side Bun artifact builder.
 */
const DEFAULT_BUN_ALIAS = "bun:1.2.0";

/**
 * Builder for TypeScript/Node.js project artifacts.
 *
 * Matches the Rust SDK's `language::TypeScript` builder. Supports two build
 * modes based on the entrypoint value:
 *
 * - **Binary mode** (entrypoint set via `withEntrypoint`): Compiles the
 *   project to a standalone binary using `bun build --compile`. Output is
 *   placed at `$VORPAL_OUTPUT/bin/{name}`.
 *
 * - **Library mode** (entrypoint undefined, the default): Builds the project
 *   using `bun x tsc --project tsconfig.json --outDir dist` and copies
 *   `package.json`, `dist/`, and `node_modules/` to `$VORPAL_OUTPUT/`.
 *
 * By default, the builder fetches the Vorpal SDK TypeScript library
 * (`library/vorpal-sdk-typescript:latest`) and adds it as both an artifact
 * dependency and a node module (`@vorpal/sdk`). Disable with
 * `.withVorpalSdk(false)`.
 *
 * Usage (binary mode):
 * ```typescript
 * const digest = await new TypeScript("my-app", SYSTEMS)
 *   .withEntrypoint("src/index.ts")
 *   .withIncludes(["src", "package.json", "tsconfig.json", "bun.lock"])
 *   .build(context);
 * ```
 *
 * Usage (library mode):
 * ```typescript
 * const digest = await new TypeScript("my-lib", SYSTEMS)
 *   .withIncludes(["src", "package.json", "tsconfig.json", "bun.lock"])
 *   .build(context);
 * ```
 */
export class TypeScript {
  private _aliases: string[] = [];
  private _artifacts: string[] = [];
  private _entrypoint: string | undefined = undefined;
  private _environments: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _nodeModules: Map<string, string> = new Map();
  private _secrets: ArtifactStepSecret[] = [];
  private _sourceScripts: string[] = [];
  private _systems: ArtifactSystem[];
  private _vorpalSdk: boolean = true;
  private _workingDir: string | undefined = undefined;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Sets artifact aliases. Duplicates are ignored.
   *
   * Note: aliases are accepted for API parity with the Rust SDK but are
   * not yet wired through to Artifact.build (matching Rust SDK limitation).
   */
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
   * Maps multiple npm packages to Vorpal store artifacts.
   * At build time, package.json and bun.lock are rewritten to point
   * dependency paths to the artifact environment keys.
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
   * Controls whether the Vorpal SDK TypeScript library is automatically
   * fetched and added as a dependency. Defaults to true.
   *
   * When true, fetches `library/vorpal-sdk-typescript:latest` and adds
   * it to both artifacts and node_modules as `@vorpal/sdk`.
   */
  withVorpalSdk(include: boolean): this {
    this._vorpalSdk = include;
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

    // Setup Vorpal SDK -- must happen BEFORE node module script generation
    if (this._vorpalSdk) {
      const vorpalSdk = await context.fetchArtifactAlias(
        "library/vorpal-sdk-typescript:latest",
      );
      this._artifacts.push(vorpalSdk);
      this._nodeModules.set("@vorpal/sdk", vorpalSdk);
    }

    // Sort node modules alphabetically by package name for deterministic output
    const sortedNodeModules = [...this._nodeModules.entries()].sort((a, b) =>
      a[0].localeCompare(b[0]),
    );

    // Setup node modules -- package.json rewriting script
    const stepPackageJsonJsParts: string[] = [];
    stepPackageJsonJsParts.push("const fs=require('fs')");
    stepPackageJsonJsParts.push(
      "const p=JSON.parse(fs.readFileSync('package.json','utf8'))",
    );

    for (const [packageName, digest] of sortedNodeModules) {
      const envKey = getEnvKey(digest);
      stepPackageJsonJsParts.push(
        `if(p.dependencies?.['${packageName}'])p.dependencies['${packageName}']='file:${envKey}'`,
      );
      stepPackageJsonJsParts.push(
        `if(p.devDependencies?.['${packageName}'])p.devDependencies['${packageName}']='file:${envKey}'`,
      );
    }

    stepPackageJsonJsParts.push(
      "fs.writeFileSync('package.json',JSON.stringify(p,null,2))",
    );

    const stepPackageJsonJs = stepPackageJsonJsParts.join(";") + ";";
    const stepPackageJsonScript = `${bunBin}/bun -e "${stepPackageJsonJs}"\n`;

    // Setup node modules -- bun.lock rewriting script
    const stepBunLockJsParts: string[] = [];
    stepBunLockJsParts.push("const fs=require('fs')");
    stepBunLockJsParts.push(
      "if(fs.existsSync('bun.lock')){var t=fs.readFileSync('bun.lock','utf8');var q=String.fromCharCode(34)",
    );

    for (const [packageName, digest] of sortedNodeModules) {
      const envKey = getEnvKey(digest);

      // Replace workspace dependency value: "package": "file:/old" -> "package": "file:<env_key>"
      stepBunLockJsParts.push(
        `var p1=q+'${packageName}'+q+': '+q+'file:';var i=t.indexOf(p1);while(i>=0){var s=i+p1.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'${envKey}'+t.substring(e);i=t.indexOf(p1,s)}`,
      );

      // Replace packages resolved specifier: "package@file:/old" -> "package@file:<env_key>"
      stepBunLockJsParts.push(
        `var p2=q+'${packageName}@file:';var i=t.indexOf(p2);while(i>=0){var s=i+p2.length;var e=t.indexOf(q,s);t=t.substring(0,s)+'${envKey}'+t.substring(e);i=t.indexOf(p2,s)}`,
      );
    }

    stepBunLockJsParts.push("fs.writeFileSync('bun.lock',t)}");

    const stepBunLockJs = stepBunLockJsParts.join(";") + ";";
    const stepBunLockScript = `${bunBin}/bun -e "${stepBunLockJs}"\n`;

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
${stepPackageJsonScript}
${stepBunLockScript}

${bunBin}/bun install --frozen-lockfile

${stepBuildCommand}`;

    // Build environment variables
    const stepEnvironments = [`PATH=${bunBin}`, ...this._environments];

    // Build artifact dependencies
    const stepArtifacts = [bunDigest, ...this._artifacts];

    // Add node module artifact digests
    for (const [, digest] of sortedNodeModules) {
      stepArtifacts.push(digest);
    }

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
  private _nodeModules: Map<string, string> = new Map();
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
   * Makes a TypeScript package artifact available in the dev environment.
   * Sets NODE_PATH to include the artifact's parent directory so that
   * Bun/Node.js can resolve the package.
   *
   * @param packageName - npm package name (e.g., "@vorpal/sdk")
   * @param digest - Artifact digest for the package
   */
  withNodeModule(packageName: string, digest: string): this {
    this._nodeModules.set(packageName, digest);
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

    // Add node module artifacts and NODE_PATH entries
    if (this._nodeModules.size > 0) {
      // Sort alphabetically by package name for deterministic output
      const sortedNodeModules = [...this._nodeModules.entries()].sort((a, b) =>
        a[0].localeCompare(b[0])
      );

      const nodePaths: string[] = [];
      for (const [, digest] of sortedNodeModules) {
        artifacts.push(digest);
        nodePaths.push(`${getEnvKey(digest)}/..`);
      }

      environments.push(`NODE_PATH=${nodePaths.join(":")}`);
    }

    let devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.length > 0) {
      devenv = devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}

