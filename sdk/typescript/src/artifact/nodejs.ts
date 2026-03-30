import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Builder for the Node.js runtime artifact.
 *
 * Mirrors Rust `NodeJS` struct in `sdk/rust/src/artifact/nodejs.rs`.
 * Downloads and extracts the official Node.js binary distribution.
 */
export class NodeJS {
  async build(context: ConfigContext): Promise<string> {
    const name = "nodejs";
    const system = context.getSystem();

    let sourceTarget: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "darwin-arm64";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux-arm64";
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

    const sourceVersion = "22.22.0";
    const sourcePath = `https://nodejs.org/dist/v${sourceVersion}/node-v${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `cp -pr "./source/${name}/node-v${sourceVersion}-${sourceTarget}/." "$VORPAL_OUTPUT"`;
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
