import type { ConfigContext } from "../context.js";
import { sourceTools } from "./go.js";
import { Go } from "./language/go.js";
import { SYSTEMS } from "../system.js";

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

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:0.42.0`])
      .withBuildDirectory(buildDirectory)
      .withSource(sourceTools(name))
      .build(context);
  }
}
