import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

/**
 * Builder for the protoc (Protocol Buffers compiler) artifact.
 *
 * Mirrors Rust `Protoc` struct in `sdk/rust/src/artifact/protoc.rs`.
 * Downloads and extracts the protoc binary from a zip archive.
 */
export class Protoc {
  async build(context: ConfigContext): Promise<string> {
    const name = "protoc";
    const system = getSystemStr(context.getSystem());

    let sourceTarget: string;

    switch (system) {
      case "aarch64-darwin":
        sourceTarget = "osx-aarch_64";
        break;
      case "aarch64-linux":
        sourceTarget = "linux-aarch_64";
        break;
      case "x86_64-darwin":
        sourceTarget = "osx-x86_64";
        break;
      case "x86_64-linux":
        sourceTarget = "linux-x86_64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = "34.0";
    const sourcePath = `https://sdk.vorpal.build/source/protoc-${sourceVersion}-${sourceTarget}.zip`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/${name}/bin/protoc" "$VORPAL_OUTPUT/bin/protoc"

chmod +x "$VORPAL_OUTPUT/bin/protoc"`;

    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
