import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

/**
 * Builder for the Node.js runtime artifact.
 *
 * Mirrors Rust `NodeJS` struct in `sdk/rust/src/artifact/nodejs.rs`.
 * Downloads and extracts the official Node.js binary distribution.
 */
export class NodeJS {
  async build(context: ConfigContext): Promise<string> {
    const name = "nodejs";
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

    const sourceVersion = "22.22.0";
    const sourcePath = `https://sdk.vorpal.build/source/node-v${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `cp -pr "./source/${name}/node-v${sourceVersion}-${sourceTarget}/." "$VORPAL_OUTPUT"`;
    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
