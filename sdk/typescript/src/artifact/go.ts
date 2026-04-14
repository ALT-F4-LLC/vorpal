import type {
  ArtifactSource as ArtifactSourceMsg,
} from "../api/artifact/artifact.js";
import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Creates a shared ArtifactSource for the Go tools repository.
 * Used by goimports and gopls which both build from the same source.
 *
 * Mirrors Rust `go::source_tools()` in `sdk/rust/src/artifact/go.rs`.
 */
export function sourceTools(name: string): ArtifactSourceMsg {
  const version = "0.42.0";
  const path = `https://sdk.vorpal.build/source/go-tools-v${version}.tar.gz`;
  return new ArtifactSource(name, path).build();
}

// ---------------------------------------------------------------------------
// Go Distribution
// ---------------------------------------------------------------------------

/**
 * Builder for the Go distribution artifact.
 *
 * Mirrors Rust `Go` struct in `sdk/rust/src/artifact/go.rs`.
 * Downloads and extracts the official Go binary distribution.
 */
export class GoBin {
  async build(context: ConfigContext): Promise<string> {
    const name = "go";
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
        sourceTarget = "darwin-amd64";
        break;
      case ArtifactSystem.X8664_LINUX:
        sourceTarget = "linux-amd64";
        break;
      default:
        throw new Error(`unsupported ${name} system: ${system}`);
    }

    const sourceVersion = "1.26.0";
    const sourcePath = `https://sdk.vorpal.build/source/go${sourceVersion}.${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `cp -pr "./source/${name}/go/." "$VORPAL_OUTPUT"`;
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
