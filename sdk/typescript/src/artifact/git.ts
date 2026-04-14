import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/**
 * Builder for the Git artifact.
 *
 * Mirrors Rust `Git` struct in `sdk/rust/src/artifact/git.rs`.
 * Downloads tarball, runs configure+make to build from source.
 */
export class Git {
  async build(context: ConfigContext): Promise<string> {
    const name = "git";

    const sourceVersion = "2.53.0";

    const sourcePath = `https://sdk.vorpal.build/source/git-${sourceVersion}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"

pushd ./source/${name}/git-${sourceVersion}

./configure --prefix=$VORPAL_OUTPUT

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
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
