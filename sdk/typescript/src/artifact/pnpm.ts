import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

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
    const system = getSystemStr(context.getSystem());

    let sourceTarget: string;

    switch (system) {
      case "aarch64-darwin":
        sourceTarget = "macos-arm64";
        break;
      case "aarch64-linux":
        sourceTarget = "linux-arm64";
        break;
      case "x86_64-darwin":
        sourceTarget = "macos-x64";
        break;
      case "x86_64-linux":
        sourceTarget = "linux-x64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/pnpm-${sourceVersion}-${sourceTarget}`;

    const source = new ArtifactSource(name, sourcePath).build();

    // macos-arm64 ships with the version in the filename; all other targets do not.
    const sourceFilename = `pnpm-${sourceVersion}-${sourceTarget}`;

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/${name}/${sourceFilename}" "$VORPAL_OUTPUT/bin/pnpm"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`;

    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
