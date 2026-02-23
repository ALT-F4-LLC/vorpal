import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import { DevelopmentEnvironment } from "../../artifact.js";

const DEFAULT_BUN_ALIAS = "bun:1.2.0";

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
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Adds extra environment variables beyond what DevelopmentEnvironment provides.
   * Format: "KEY=VALUE".
   */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  /** Adds secrets available during the environment build step. */
  withSecrets(secrets: Array<[string, string]>): this {
    this._secrets = secrets;
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

    const devenv = new DevelopmentEnvironment(this._name, this._systems)
      .withArtifacts(artifacts)
      .withEnvironments(environments);

    if (this._secrets.length > 0) {
      devenv.withSecrets(this._secrets);
    }

    return devenv.build(context);
  }
}
