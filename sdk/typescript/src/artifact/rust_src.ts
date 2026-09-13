import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { RUST_TOOLCHAIN_VERSION } from "./rust_toolchain.js";
import { SYSTEMS } from "../system.js";

/**
 * Builder for the RustSrc artifact.
 *
 * Mirrors Rust `RustSrc` struct in `sdk/rust/src/artifact/rust_src.rs`.
 * Downloads and extracts the Rust source from static.rust-lang.org.
 * Note: rust-src is platform-independent (no target in URL).
 */
export class RustSrc {
  async build(context: ConfigContext): Promise<string> {
    const name = "rust-src";
    const sourceVersion = RUST_TOOLCHAIN_VERSION;
    const sourcePath = `https://sdk.vorpal.build/source/rust-src-${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `cp -pr "./source/${name}/${name}-${sourceVersion}/${name}/." "$VORPAL_OUTPUT"`;
    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withSources([source])
      .build(context);
  }
}
