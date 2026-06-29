import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";

/// Canonical conforming copy of the build-target CPython interpreter pin.
///
/// Operator rule: CPython 3.13, latest patch. Concrete pin: 3.13.14 from
/// python-build-standalone release tag `20260623`, `install_only` (relocatable).
/// Byte-equal to the Rust canonical in sdk/rust/src/artifact/cpython.rs
/// (ADR 0001 Part B; DKT-19 asserts byte-equality).
export const DEFAULT_PYTHON_VERSION = "3.13.14";

/**
 * Maps a Vorpal ArtifactSystem to the python-build-standalone target triple.
 *
 * Mirrors Rust `cpython::target()` — name-agnostic error message on unknown system.
 */
export function cpythonTarget(system: ArtifactSystem): string {
  switch (system) {
    case ArtifactSystem.AARCH64_DARWIN:
      return "aarch64-apple-darwin";
    case ArtifactSystem.AARCH64_LINUX:
      return "aarch64-unknown-linux-gnu";
    case ArtifactSystem.X8664_DARWIN:
      return "x86_64-apple-darwin";
    case ArtifactSystem.X8664_LINUX:
      return "x86_64-unknown-linux-gnu";
    default:
      throw new Error(`unsupported toolchain target system: ${system}`);
  }
}

/**
 * Builder for the CPython interpreter artifact (python-build-standalone, relocatable install_only).
 *
 * Mirrors Rust `Cpython` struct in `sdk/rust/src/artifact/cpython.rs`.
 *
 * Source name is `cpython`, NOT `python` — the repo's existing linux_vorpal bootstrap
 * already owns a `python` source (compiled from source), and sources key by
 * `(name, platform)`, so reusing `python` would collide (ADR 0001 / TDD §4, H1).
 *
 * PROVENANCE — no inline `withDigest` (ADR 0001 Part A). The canonical pin is the
 * per-triple `Vorpal.lock` entry captured via `--unlock`, mirroring bun/nodejs/cargo.
 * Until that capture lands, the HTTP source is intentionally unpinned: the C1 mint gate
 * fails the build closed with "unpinned - use --unlock", which is the true state. We do
 * NOT set a placeholder digest — `agent.rs` resolves an inline digest against the registry
 * cache (`check`) and returns it on a hit WITHOUT downloading or verifying content, so a
 * predictable placeholder would be a pre-seedable cache-poison key.
 */
export class Cpython {
  private version: string;

  constructor() {
    this.version = DEFAULT_PYTHON_VERSION;
  }

  withVersion(version: string): this {
    this.version = version;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const name = "cpython";
    const system = context.getSystem();

    const sourceTarget = cpythonTarget(system);
    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/cpython-${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    // pbs install_only tarballs unpack to a top-level python/ dir at the tarball root.
    const stepScript = `mkdir -p "$VORPAL_OUTPUT"
cp -prf "./source/${name}/python/." "$VORPAL_OUTPUT/"
`;
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
