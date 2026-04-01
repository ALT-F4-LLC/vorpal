import type {
  ArtifactSource as ArtifactSourceMsg,
} from "../../api/artifact/artifact.js";
import { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import {
  Artifact,
  ArtifactSource,
  DevelopmentEnvironment,
  getEnvKey,
  secretsToProto,
} from "../../artifact.js";
import { Git } from "../git.js";
import { GoBin } from "../go.js";
import { Goimports } from "../goimports.js";
import { Gopls } from "../gopls.js";
import { Protoc } from "../protoc.js";
import { ProtocGenGo } from "../protoc_gen_go.js";
import { ProtocGenGoGrpc } from "../protoc_gen_go_grpc.js";
import { Staticcheck } from "../staticcheck.js";
import { shell } from "../step.js";

// ---------------------------------------------------------------------------
// System mapping helpers
// ---------------------------------------------------------------------------

/**
 * Maps an ArtifactSystem enum to the Go `GOOS` value.
 * Matches `get_goos()` in `sdk/rust/src/artifact/language/go.rs`
 * and `GetGOOS()` in `sdk/go/pkg/artifact/language/go.go`.
 */
export function getGoos(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
    case ArtifactSystem.X8664_DARWIN:
      return "darwin";
    case ArtifactSystem.AARCH64_LINUX:
    case ArtifactSystem.X8664_LINUX:
      return "linux";
    default:
      throw new Error(`unsupported 'go' system: ${system}`);
  }
}

/**
 * Maps an ArtifactSystem enum to the Go `GOARCH` value.
 * Matches `get_goarch()` in `sdk/rust/src/artifact/language/go.rs`
 * and `GetGOARCH()` in `sdk/go/pkg/artifact/language/go.go`.
 */
export function getGoarch(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
    case ArtifactSystem.AARCH64_LINUX:
      return "arm64";
    case ArtifactSystem.X8664_DARWIN:
    case ArtifactSystem.X8664_LINUX:
      return "amd64";
    default:
      throw new Error(`unsupported 'go' system: ${system}`);
  }
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

/**
 * Builder for Go project artifacts.
 *
 * Analogous to:
 * - Rust SDK: `sdk/rust/src/artifact/language/go.rs` (`Go` struct)
 * - Go SDK: `sdk/go/pkg/artifact/language/go.go` (`Go` struct)
 *
 * The builder:
 * 1. Fetches the Go distribution artifact from the registry
 * 2. Fetches the Git artifact from the registry
 * 3. Builds source using ArtifactSource (or uses an explicit source)
 * 4. Computes GOOS and GOARCH from the target system
 * 5. Generates a build script that runs `go build`
 * 6. Creates a shell step and returns the artifact digest
 *
 * Usage:
 * ```typescript
 * const digest = await new Go("my-go-app", SYSTEMS)
 *   .withIncludes(["cmd", "pkg", "go.mod", "go.sum"])
 *   .build(context);
 * ```
 */
export class Go {
  private _aliases: string[] = [];
  private _artifacts: string[] = [];
  private _buildDirectory: string = ".";
  private _buildFlags: string = "";
  private _buildPath: string = ".";
  private _environments: string[] = [];
  private _includes: string[] = [];
  private _name: string;
  private _secrets: Map<string, string> = new Map();
  private _source: ArtifactSourceMsg | undefined = undefined;
  private _sourceScripts: string[] = [];
  private _systems: ArtifactSystem[];

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /** Adds human-readable aliases for this artifact. */
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
   * Sets the build directory passed as `-C` flag to `go build`.
   * Defaults to `"."`.
   */
  withBuildDirectory(directory: string): this {
    this._buildDirectory = directory;
    return this;
  }

  /**
   * Sets additional flags for `go build`.
   * Defaults to `""`.
   */
  withBuildFlags(flags: string): this {
    this._buildFlags = flags;
    return this;
  }

  /**
   * Sets the build path argument for `go build`.
   * Defaults to `"."`.
   */
  withBuildPath(path: string): this {
    this._buildPath = path;
    return this;
  }

  /**
   * Sets environment variables for the build step.
   * Format: `"KEY=VALUE"`.
   */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
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
   *
   * @param secrets - Map of secret name to value
   */
  withSecrets(secrets: Map<string, string>): this {
    for (const [k, v] of secrets) {
      if (!this._secrets.has(k)) {
        this._secrets.set(k, v);
      }
    }
    return this;
  }

  /**
   * Sets an explicit ArtifactSource for the project.
   * If not set, one is constructed from the name with any includes.
   */
  withSource(source: ArtifactSourceMsg): this {
    this._source = source;
    return this;
  }

  /**
   * Adds a script to run inside the source directory before the build.
   * Multiple scripts are deduplicated and joined with newlines.
   */
  withSourceScript(script: string): this {
    if (!this._sourceScripts.includes(script)) {
      this._sourceScripts.push(script);
    }
    return this;
  }

  /**
   * Builds the Go project artifact.
   *
   * CRITICAL: The build script must be character-for-character identical
   * to what the Rust SDK and Go SDK produce for the same inputs.
   *
   * @returns The artifact digest string
   */
  async build(context: ConfigContext): Promise<string> {
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

      source = sourceBuilder.build();
    }

    const sourceDir = `./source/${source.name}`;

    // Build step script -- mirrors Rust SDK formatdoc! in go.rs lines 147-174
    //
    // The script is built incrementally to match the Rust SDK's formatdoc!
    // concatenation pattern exactly:
    //   1. pushd + mkdir
    //   2. (optional) source scripts
    //   3. go build + go clean

    let stepScript = `pushd ${sourceDir}\n\nmkdir -p $VORPAL_OUTPUT/bin`;

    if (this._sourceScripts.length > 0) {
      const sourceScripts = this._sourceScripts.join("\n");
      stepScript = `${stepScript}\n\n${sourceScripts}`;
    }

    stepScript =
      `${stepScript}\n\n` +
      `go build -C ${this._buildDirectory} -o $VORPAL_OUTPUT/bin/${this._name} ${this._buildFlags} ${this._buildPath}\n\n` +
      `go clean -modcache`;

    // Build tool artifacts
    const git = await new Git().build(context);
    const go = await new GoBin().build(context);

    // Compute GOOS and GOARCH
    const goarch = getGoarch(context.getSystem());
    const goos = getGoos(context.getSystem());

    // Build step environments
    const stepEnvironments: string[] = [
      `GOARCH=${goarch}`,
      "GOCACHE=$VORPAL_WORKSPACE/go/cache",
      `GOOS=${goos}`,
      "GOPATH=$VORPAL_WORKSPACE/go",
      `PATH=${getEnvKey(go)}/bin`,
    ];

    for (const env of this._environments) {
      stepEnvironments.push(env);
    }

    // Create step
    const stepArtifacts = [git, go, ...this._artifacts];

    const step = await shell(
      context,
      stepArtifacts,
      stepEnvironments,
      stepScript,
      secretsToProto(this._secrets),
    );

    // Create and return artifact
    return new Artifact(this._name, [step], this._systems)
      .withAliases(this._aliases)
      .withSources([source])
      .build(context);
  }
}

// ---------------------------------------------------------------------------
// Go Development Environment
// ---------------------------------------------------------------------------

/**
 * Builder for Go development environment artifacts.
 *
 * Provides a pre-configured Go development environment with standard
 * tooling (Go, Git, goimports, gopls, protoc, staticcheck) and
 * platform-specific environment variables (CGO_ENABLED, GOARCH, GOOS).
 *
 * Usage:
 * ```typescript
 * const digest = await new GoDevelopmentEnvironment("my-shell", SYSTEMS)
 *   .build(context);
 * ```
 */
export class GoDevelopmentEnvironment {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _secrets: Map<string, string> = new Map();
  private _systems: ArtifactSystem[];

  // Flags to include/exclude optional default tools
  private _includeProtoc: boolean = true;
  private _includeProtocGenGo: boolean = true;
  private _includeProtocGenGoGrpc: boolean = true;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Adds extra artifact dependencies beyond the default Go tooling.
   * These are appended to the default artifacts, not replacing them.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts.push(...artifacts);
    return this;
  }

  /**
   * Adds extra environment variables beyond the default Go environment.
   * Format: "KEY=VALUE".
   * These are appended to the defaults (CGO_ENABLED, GOARCH, GOOS).
   */
  withEnvironments(environments: string[]): this {
    this._environments.push(...environments);
    return this;
  }

  /** Exclude protoc and its Go plugins from the default tooling. */
  withoutProtoc(): this {
    this._includeProtoc = false;
    this._includeProtocGenGo = false;
    this._includeProtocGenGoGrpc = false;
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
   * Builds the Go development environment artifact.
   *
   * Default artifacts fetched:
   * - go (Go toolchain)
   * - git
   * - goimports
   * - gopls (Go language server)
   * - protoc (if not excluded)
   * - protoc-gen-go (if not excluded)
   * - protoc-gen-go-grpc (if not excluded)
   * - staticcheck
   *
   * Default environment variables:
   * - CGO_ENABLED=0
   * - GOARCH={platform-specific}
   * - GOOS={platform-specific}
   */
  async build(context: ConfigContext): Promise<string> {
    // Build default tool artifacts
    const go = await new GoBin().build(context);
    const git = await new Git().build(context);
    const goimports = await new Goimports().build(context);
    const gopls = await new Gopls().build(context);
    const staticcheck = await new Staticcheck().build(context);

    const artifacts: string[] = [git, go, goimports, gopls];

    if (this._includeProtoc) {
      const protoc = await new Protoc().build(context);
      artifacts.push(protoc);
    }

    if (this._includeProtocGenGo) {
      const protocGenGo = await new ProtocGenGo().build(context);
      artifacts.push(protocGenGo);
    }

    if (this._includeProtocGenGoGrpc) {
      const protocGenGoGrpc = await new ProtocGenGoGrpc().build(context);
      artifacts.push(protocGenGoGrpc);
    }

    artifacts.push(staticcheck);
    artifacts.push(...this._artifacts);

    // Compute platform-specific environment variables
    const goarch = getGoarch(context.getSystem());
    const goos = getGoos(context.getSystem());

    const environments: string[] = [
      "CGO_ENABLED=0",
      `GOARCH=${goarch}`,
      `GOOS=${goos}`,
      ...this._environments,
    ];

    // Delegate to DevelopmentEnvironment
    let devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.size > 0) {
      devenv = devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}
