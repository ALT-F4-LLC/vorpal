import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

export const DEFAULT_PNPM_VERSION = "12.21.0";

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
    const system = getSystemStr(context.getSystem());

    let sourceTarget: string;

    switch (system) {
      case "aarch64-darwin":
        sourceTarget = "darwin-arm64";
        break;
      case "aarch64-linux":
        sourceTarget = "linux-arm64";
        break;
      case "x86_64-darwin":
        sourceTarget = "darwin-x64";
        break;
      case "x86_64-linux":
        sourceTarget = "linux-x64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/pnpm-${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    // The archive holds the pnpm executable and the dist tree it loads from
    // its own directory, so both land in bin, as pnpm's installer lays out
    // PNPM_HOME.
    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -pR "./source/${name}/." "$VORPAL_OUTPUT/bin/"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`;

    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
