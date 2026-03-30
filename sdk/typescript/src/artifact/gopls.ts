import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { sourceTools } from "./go.js";
import { Go } from "./language/go.js";

/**
 * Builder for the gopls artifact.
 *
 * Mirrors Rust `Gopls` struct in `sdk/rust/src/artifact/gopls.rs`.
 * Builds gopls from the Go tools source using the Go language builder.
 */
export class Gopls {
  async build(context: ConfigContext): Promise<string> {
    const name = "gopls";

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Go(name, systems)
      .withAliases([`${name}:0.42.0`])
      .withBuildDirectory(name)
      .withSource(sourceTools(name))
      .build(context);
  }
}
