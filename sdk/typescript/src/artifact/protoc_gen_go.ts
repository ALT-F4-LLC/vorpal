import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Builder for the protoc-gen-go artifact.
 *
 * Mirrors Rust `ProtocGenGo` struct in `sdk/rust/src/artifact/protoc_gen_go.rs`.
 * Downloads and extracts the protoc-gen-go binary from a tar.gz archive.
 */
export class ProtocGenGo {
  async build(context: ConfigContext): Promise<string> {
    const name = "protoc-gen-go";
    const system = context.getSystem();

    let sourceTarget: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "darwin.arm64";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux.arm64";
        break;
      case ArtifactSystem.X8664_DARWIN:
        sourceTarget = "darwin.amd64";
        break;
      case ArtifactSystem.X8664_LINUX:
        sourceTarget = "linux.amd64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = "1.36.11";
    const sourcePath = `https://sdk.vorpal.build/source/protoc-gen-go.v${sourceVersion}.${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/protoc-gen-go/protoc-gen-go" "$VORPAL_OUTPUT/bin/protoc-gen-go"

chmod +x "$VORPAL_OUTPUT/bin/protoc-gen-go"`;

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
