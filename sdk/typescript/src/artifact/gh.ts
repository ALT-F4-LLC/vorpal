import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

export const DEFAULT_GH_VERSION = "2.87.3";

/**
 * Builder for the GitHub CLI artifact.
 *
 * Mirrors Rust `Gh` struct in `sdk/rust/src/artifact/gh.rs`.
 * Downloads and extracts the gh binary from a release archive.
 */
export class Gh {
  async build(context: ConfigContext): Promise<string> {
    const name = "gh";
    const system = context.getSystem();

    let sourceTarget: string;
    let sourceExtension: string;

    switch (system) {
      case ArtifactSystem.AARCH64_DARWIN:
        sourceTarget = "macOS_arm64";
        sourceExtension = "zip";
        break;
      case ArtifactSystem.AARCH64_LINUX:
        sourceTarget = "linux_arm64";
        sourceExtension = "tar.gz";
        break;
      case ArtifactSystem.X8664_DARWIN:
        sourceTarget = "macOS_amd64";
        sourceExtension = "zip";
        break;
      case ArtifactSystem.X8664_LINUX:
        sourceTarget = "linux_amd64";
        sourceExtension = "tar.gz";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = DEFAULT_GH_VERSION;
    const sourcePath = `https://sdk.vorpal.build/source/gh_${sourceVersion}_${sourceTarget}.${sourceExtension}`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/${name}/gh_${sourceVersion}_${sourceTarget}/bin/gh" "$VORPAL_OUTPUT/bin/gh"

chmod +x "$VORPAL_OUTPUT/bin/gh"
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
