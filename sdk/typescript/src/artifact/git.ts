import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { SYSTEMS } from "../system.js";

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

./configure --prefix=/

make_flags="RUNTIME_PREFIX=YesPlease gitexecdir=libexec/git-core template_dir=share/git-core/templates sysconfdir=etc"

make $make_flags
make $make_flags DESTDIR=$VORPAL_OUTPUT install`;

    const steps = [await shell(context, [], [], stepScript, [])];

    return new Artifact(name, steps, SYSTEMS)
      .withAliases([`${name}:${sourceVersion}`])
      .withSources([source])
      .build(context);
  }
}
