import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Builder for the protoc (Protocol Buffers compiler) artifact.
 *
 * Mirrors Rust `Protoc` struct in `sdk/rust/src/artifact/protoc.rs`.
 * Downloads and extracts the protoc binary from a zip archive.
 */
export class Protoc {
  async build(context: ConfigContext): Promise<string> {
    const name = "protoc";
    const system = context.getSystem();

    let sourceTarget: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "osx-aarch_64";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux-aarch_64";
        break;
      case ArtifactSystem.X8664_DARWIN:
        sourceTarget = "osx-x86_64";
        break;
      case ArtifactSystem.X8664_LINUX:
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
