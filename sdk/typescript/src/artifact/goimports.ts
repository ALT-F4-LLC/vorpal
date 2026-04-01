import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { sourceTools } from "./go.js";
import { Go } from "./language/go.js";

/**
 * Builder for the goimports artifact.
 *
 * Mirrors Rust `Goimports` struct in `sdk/rust/src/artifact/goimports.rs`.
 * Builds goimports from the Go tools source using the Go language builder.
 */
export class Goimports {
  async build(context: ConfigContext): Promise<string> {
    const name = "goimports";

    const buildDirectory = `cmd/${name}`;

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:0.42.0`])
      .withBuildDirectory(buildDirectory)
      .withSource(sourceTools(name))
      .build(context);
  }
}
