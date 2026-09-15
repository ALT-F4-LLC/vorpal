import type { ConfigContext } from "../context.js";
import { ArtifactSource } from "../artifact.js";
import { Go } from "./language/go.js";
import { SYSTEMS } from "../system.js";

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

    const sourcePath = `https://sdk.vorpal.build/source/crane-v${version}.tar.gz`;
    const source = new ArtifactSource(name, sourcePath).build();

    const buildDirectory = `./go-containerregistry-${version}`;
    const buildPath = `./cmd/${name}`;

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:${version}`])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
