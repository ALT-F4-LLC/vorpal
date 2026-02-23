import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import { DevelopmentEnvironment, getEnvKey } from "../../artifact.js";
import { getGoarch, getGoos } from "../language/go.js";

// Default tool aliases -- centralized so version bumps happen in one place
const DEFAULT_GO_ALIAS = "go:1.24.2";
const DEFAULT_GIT_ALIAS = "git:2.52.0";
const DEFAULT_GOIMPORTS_ALIAS = "goimports:0.29.0";
const DEFAULT_GOPLS_ALIAS = "gopls:0.29.0";
const DEFAULT_PROTOC_ALIAS = "protoc:25.4";
const DEFAULT_PROTOC_GEN_GO_ALIAS = "protoc-gen-go:1.36.3";
const DEFAULT_PROTOC_GEN_GO_GRPC_ALIAS = "protoc-gen-go-grpc:1.70.0";
const DEFAULT_STATICCHECK_ALIAS = "staticcheck:2025.1.1";

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
  private _secrets: Array<[string, string]> = [];
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
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Adds extra environment variables beyond the default Go environment.
   * Format: "KEY=VALUE".
   * These are appended to the defaults (CGO_ENABLED, GOARCH, GOOS).
   */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
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
  withSecrets(secrets: Array<[string, string]>): this {
    this._secrets = secrets;
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
    // Fetch default tool artifacts
    const go = await context.fetchArtifactAlias(DEFAULT_GO_ALIAS);
    const git = await context.fetchArtifactAlias(DEFAULT_GIT_ALIAS);
    const goimports = await context.fetchArtifactAlias(DEFAULT_GOIMPORTS_ALIAS);
    const gopls = await context.fetchArtifactAlias(DEFAULT_GOPLS_ALIAS);
    const staticcheck = await context.fetchArtifactAlias(DEFAULT_STATICCHECK_ALIAS);

    const artifacts: string[] = [git, go, goimports, gopls];

    if (this._includeProtoc) {
      const protoc = await context.fetchArtifactAlias(DEFAULT_PROTOC_ALIAS);
      artifacts.push(protoc);
    }

    if (this._includeProtocGenGo) {
      const protocGenGo = await context.fetchArtifactAlias(DEFAULT_PROTOC_GEN_GO_ALIAS);
      artifacts.push(protocGenGo);
    }

    if (this._includeProtocGenGoGrpc) {
      const protocGenGoGrpc = await context.fetchArtifactAlias(DEFAULT_PROTOC_GEN_GO_GRPC_ALIAS);
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
    const devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.length > 0) {
      devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}
