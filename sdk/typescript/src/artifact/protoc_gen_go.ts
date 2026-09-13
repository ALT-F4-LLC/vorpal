import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { getSystemStr, SYSTEMS } from "../system.js";

/**
 * Builder for the protoc-gen-go artifact.
 *
 * Mirrors Rust `ProtocGenGo` struct in `sdk/rust/src/artifact/protoc_gen_go.rs`.
 * Downloads and extracts the protoc-gen-go binary from a tar.gz archive.
 */
export class ProtocGenGo {
  async build(context: ConfigContext): Promise<string> {
    const name = "protoc-gen-go";
    const system = getSystemStr(context.getSystem());

    let sourceTarget: string;

    switch (system) {
      case "aarch64-darwin":
        sourceTarget = "darwin.arm64";
        break;
      case "aarch64-linux":
        sourceTarget = "linux.arm64";
        break;
      case "x86_64-darwin":
        sourceTarget = "darwin.amd64";
        break;
      case "x86_64-linux":
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

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
