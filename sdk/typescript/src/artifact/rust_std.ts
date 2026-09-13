import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { RUST_TOOLCHAIN_VERSION, rustToolchainTarget } from "./rust_toolchain.js";
import { SYSTEMS } from "../system.js";

/**
 * Builder for the RustStd artifact.
 *
 * Mirrors Rust `RustStd` struct in `sdk/rust/src/artifact/rust_std.rs`.
 * Downloads and extracts the Rust standard library from static.rust-lang.org.
 */
export class RustStd {
  async build(context: ConfigContext): Promise<string> {
    const name = "rust-std";
    const system = context.getSystem();

    const sourceTarget = rustToolchainTarget(system);
    const sourceVersion = RUST_TOOLCHAIN_VERSION;
    const sourcePath = `https://sdk.vorpal.build/source/${name}-${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `cp -pr "./source/${name}/${name}-${sourceVersion}-${sourceTarget}/${name}-${sourceTarget}/." "$VORPAL_OUTPUT"`;
    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withSources([source])
      .build(context);
  }
}
