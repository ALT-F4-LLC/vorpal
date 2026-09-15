import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

export const DEFAULT_BUN_VERSION = "1.3.10";

/**
 * Builder for the Bun runtime artifact.
 *
 * Mirrors Rust `Bun` struct in `sdk/rust/src/artifact/bun.rs`.
 * Downloads and extracts the Bun binary from a zip archive.
 */
export class Bun {
  private version: string;

  constructor() {
    this.version = DEFAULT_BUN_VERSION;
  }

  withVersion(version: string): this {
    this.version = version;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const name = "bun";
    const system = getSystemStr(context.getSystem());

    let sourceTarget: string;

    switch (system) {
      case "aarch64-darwin":
        sourceTarget = "darwin-aarch64";
        break;
      case "aarch64-linux":
        sourceTarget = "linux-aarch64";
        break;
      case "x86_64-darwin":
        sourceTarget = "darwin-x64";
        break;
      case "x86_64-linux":
        sourceTarget = "linux-x64-baseline";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/bun-${sourceVersion}-${sourceTarget}.zip`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/${name}/bun-${sourceTarget}/bun" "$VORPAL_OUTPUT/bin/bun"
chmod +x "$VORPAL_OUTPUT/bin/bun"
`;
    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
