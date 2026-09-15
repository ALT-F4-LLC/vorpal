import type { ConfigContext } from "../context.js";
import { sourceTools } from "./go.js";
import { Go } from "./language/go.js";
import { SYSTEMS } from "../system.js";

/**
 * Builder for the gopls artifact.
 *
 * Mirrors Rust `Gopls` struct in `sdk/rust/src/artifact/gopls.rs`.
 * Builds gopls from the Go tools source using the Go language builder.
 */
export class Gopls {
  async build(context: ConfigContext): Promise<string> {
    const name = "gopls";

    return new Go(name, SYSTEMS)
      .withAliases([`${name}:0.42.0`])
      .withBuildDirectory(name)
      .withSource(sourceTools(name))
      .build(context);
  }
}
