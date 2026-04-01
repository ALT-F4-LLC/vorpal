import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

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
    const system = context.getSystem();

    let sourceTarget: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "darwin-aarch64";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux-aarch64";
        break;
      case ArtifactSystem.X8664_DARWIN:
        sourceTarget = "darwin-x64";
        break;
      case ArtifactSystem.X8664_LINUX:
        sourceTarget = "linux-x64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = this.version;
    const sourcePath = `https://github.com/oven-sh/bun/releases/download/bun-v${sourceVersion}/bun-${sourceTarget}.zip`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/${name}/bun-${sourceTarget}/bun" "$VORPAL_OUTPUT/bin/bun"
chmod +x "$VORPAL_OUTPUT/bin/bun"
`;
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
