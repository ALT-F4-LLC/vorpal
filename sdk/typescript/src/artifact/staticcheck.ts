import { ArtifactSource } from "../artifact.js";
import type { ConfigContext } from "../context.js";
import { Go } from "./language/go.js";
import { SYSTEMS } from "../system.js";

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

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withBuildDirectory(buildDirectory)
      .withBuildPath(buildPath)
      .withSource(source)
      .build(context);
  }
}
