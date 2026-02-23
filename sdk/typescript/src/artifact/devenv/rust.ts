import type { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";
import { DevelopmentEnvironment, getEnvKey } from "../../artifact.js";
import { rustToolchainTarget } from "../language/rust.js";

const RUST_TOOLCHAIN_VERSION = "1.89.0";
const PROTOC_ALIAS = "protoc:25.4";

/**
 * Builder for Rust development environment artifacts.
 *
 * Provides a pre-configured Rust development environment with the Rust
 * toolchain (cargo, rustc, clippy, rustfmt, rust-analyzer, etc.) and
 * optionally protoc. Computes platform-specific environment variables
 * (PATH, RUSTUP_HOME, RUSTUP_TOOLCHAIN) automatically.
 *
 * Usage:
 * ```typescript
 * const digest = await new RustDevelopmentEnvironment("my-shell", SYSTEMS)
 *   .build(context);
 * ```
 */
export class RustDevelopmentEnvironment {
  private _artifacts: string[] = [];
  private _environments: string[] = [];
  private _name: string;
  private _secrets: Array<[string, string]> = [];
  private _systems: ArtifactSystem[];
  private _includeProtoc: boolean = true;

  constructor(name: string, systems: ArtifactSystem[]) {
    this._name = name;
    this._systems = systems;
  }

  /**
   * Adds extra artifact dependencies beyond the default Rust tooling.
   * These are appended to the default artifacts, not replacing them.
   */
  withArtifacts(artifacts: string[]): this {
    this._artifacts = artifacts;
    return this;
  }

  /**
   * Adds extra environment variables beyond the default Rust environment.
   * Format: "KEY=VALUE".
   * These are appended to the defaults (PATH, RUSTUP_HOME, RUSTUP_TOOLCHAIN).
   */
  withEnvironments(environments: string[]): this {
    this._environments = environments;
    return this;
  }

  /** Exclude protoc from the default tooling. */
  withoutProtoc(): this {
    this._includeProtoc = false;
    return this;
  }

  /** Adds secrets available during the environment build step. */
  withSecrets(secrets: Array<[string, string]>): this {
    this._secrets = secrets;
    return this;
  }

  /**
   * Builds the Rust development environment artifact.
   *
   * Default artifacts fetched:
   * - rust-toolchain (includes cargo, rustc, clippy, rustfmt, rust-analyzer, etc.)
   * - protoc (if not excluded)
   *
   * Default environment variables:
   * - PATH={rust-toolchain}/toolchains/{version}-{target}/bin
   * - RUSTUP_HOME={rust-toolchain}
   * - RUSTUP_TOOLCHAIN={version}-{target}
   */
  async build(context: ConfigContext): Promise<string> {
    const rustToolchain = await context.fetchArtifactAlias(
      `rust-toolchain:${RUST_TOOLCHAIN_VERSION}`
    );

    const artifacts: string[] = [];

    if (this._includeProtoc) {
      const protoc = await context.fetchArtifactAlias(PROTOC_ALIAS);
      artifacts.push(protoc);
    }

    artifacts.push(rustToolchain);
    artifacts.push(...this._artifacts);

    // Compute Rust toolchain paths
    const toolchainTarget = rustToolchainTarget(context.getSystem());
    const toolchainName = `${RUST_TOOLCHAIN_VERSION}-${toolchainTarget}`;
    const toolchainBin = `${getEnvKey(rustToolchain)}/toolchains/${toolchainName}/bin`;

    const environments: string[] = [
      `PATH=${toolchainBin}`,
      `RUSTUP_HOME=${getEnvKey(rustToolchain)}`,
      `RUSTUP_TOOLCHAIN=${toolchainName}`,
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
