import { ArtifactSystem } from "../api/artifact/artifact.js";
import { ArtifactSource } from "../artifact.js";
import type { ConfigContext } from "../context.js";
import { Go } from "./language/go.js";

/**
 * Builder for the staticcheck artifact.
 *
 * Mirrors Rust `Staticcheck` struct in `sdk/rust/src/artifact/staticcheck.rs`.
 * Builds staticcheck from the go-tools source using the Go language builder.
 */
export class Staticcheck {
  async build(context: ConfigContext): Promise<string> {
    const name = "staticcheck";
    const sourceVersion = "2026.1";
    const sourcePath = `https://sdk.vorpal.build/source/staticcheck-${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `go-tools-${sourceVersion}`;
    const buildPath = `cmd/${name}/${name}.go`;

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:${sourceVersion}`])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
