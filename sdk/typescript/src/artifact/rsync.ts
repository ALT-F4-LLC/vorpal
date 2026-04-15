import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Builder for the Rsync artifact.
 *
 * Mirrors Rust `Rsync` struct in `sdk/rust/src/artifact/rsync.rs`.
 * Downloads tarball, runs configure+make with specific disable flags.
 */
export class Rsync {
  async build(context: ConfigContext): Promise<string> {
    const name = "rsync";

    const version = "3.4.1";

    const path = `https://sdk.vorpal.build/source/rsync-${version}.tar.gz`;

    const source = new ArtifactSource(name, path).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT"
pushd ./source/${name}/${name}-${version}
./configure --prefix="$VORPAL_OUTPUT" --disable-openssl --disable-xxhash --disable-zstd --disable-lz4
make
make install`;

    const steps = [await shell(context, [], [], stepScript, [])];

    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    return new Artifact(name, steps, systems)
      .withAliases([`${name}:${version}`])
      .withSources([source])
      .build(context);
  }
}
