import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

export const DEFAULT_PNPM_VERSION = "10.30.3";

/**
 * Builder for the pnpm package manager artifact.
 *
 * Mirrors Rust `Pnpm` struct in `sdk/rust/src/artifact/pnpm.rs`.
 * Downloads the pnpm binary for the target platform.
 */
export class Pnpm {
  private version: string;

  constructor() {
    this.version = DEFAULT_PNPM_VERSION;
  }

  withVersion(version: string): this {
    this.version = version;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const name = "pnpm";
    const system = context.getSystem();

    let sourceTarget: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "macos-arm64";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux-arm64";
        break;
      case ArtifactSystem.X8664_DARWIN:
        sourceTarget = "macos-x64";
        break;
      case ArtifactSystem.X8664_LINUX:
        sourceTarget = "linux-x64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/pnpm-${sourceVersion}-${sourceTarget}`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/${name}/pnpm-${sourceTarget}" "$VORPAL_OUTPUT/bin/pnpm"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`;

    const steps = [await shell(context, [], [], stepScript, [])];
    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Artifact(name, steps, systems)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
