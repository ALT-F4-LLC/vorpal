import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";

/**
 * Builder for the crane artifact.
 *
 * Mirrors Rust `Crane` struct in `sdk/rust/src/artifact/crane.rs`.
 * Builds crane from the go-containerregistry source using the Go language builder.
 */
export class Crane {
  async build(context: ConfigContext): Promise<string> {
    const name = "crane";
    const version = "0.21.1";

    const sourcePath = `https://github.com/google/go-containerregistry/archive/refs/tags/v${version}.tar.gz`;
    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `./go-containerregistry-${version}`;
    const buildPath = `./cmd/${name}`;

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:${version}`])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
