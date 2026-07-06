import { ArtifactSystem } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";
import { Artifact, ArtifactSource } from "../artifact.js";
import { shell } from "./step.js";
import { cpythonTarget } from "./cpython.js";

/// Canonical conforming copy of the build-target uv toolchain pin (Astral release).
///
/// Byte-equal to the Rust canonical in sdk/rust/src/artifact/uv.rs
/// (ADR 0001 Part B; DKT-19 asserts byte-equality).
export const DEFAULT_UV_VERSION = "0.10.11";

/**
 * Builder for the uv toolchain artifact (Astral standalone release).
 *
 * Mirrors Rust `Uv` struct in `sdk/rust/src/artifact/uv.rs`.
 *
 * HASH-ENFORCEMENT (C3 foundation): on `uv sync --frozen`, uv-0.10.11 verifies each
 * package against the per-package hashes carried in `uv.lock` and rejects any
 * content-hash mismatch. That hashed-lock verification IS the require-hashes enforcement
 * surface the build helpers wire — there is no `uv sync --require-hashes` CLI flag
 * (`--require-hashes` is uv's pip-interface flag); the enforced OUTCOME is what the C3a
 * tampered-package test proves live (TDD §Phase-2 ~L802-815).
 *
 * PROVENANCE — no inline `withDigest` (ADR 0001 Part A). The canonical pin is the
 * per-triple `Vorpal.lock` entry captured via `--unlock`, mirroring bun/nodejs/cargo.
 * Until that capture lands, the HTTP source is intentionally unpinned: the C1 mint gate
 * fails the build closed with "unpinned - use --unlock". A placeholder digest is
 * intentionally avoided — `agent.rs` returns an inline digest on a registry-cache hit
 * without verifying content, so a predictable placeholder is a pre-seedable cache-poison key.
 */
export class Uv {
  private version: string;

  constructor() {
    this.version = DEFAULT_UV_VERSION;
  }

  withVersion(version: string): this {
    this.version = version;
    return this;
  }

  async build(context: ConfigContext): Promise<string> {
    const name = "uv";
    const system = context.getSystem();

    const sourceTarget = cpythonTarget(system);
    const sourceVersion = this.version;
    const sourcePath = `https://sdk.vorpal.build/source/uv-${sourceVersion}-${sourceTarget}.tar.gz`;

    const source = new ArtifactSource(name, sourcePath).build();

    // Astral standalone release unpacks to uv-{triple}/uv at the tarball root.
    const stepScript = `mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/${name}/uv-${sourceTarget}/uv" "$VORPAL_OUTPUT/bin/uv"
chmod +x "$VORPAL_OUTPUT/bin/uv"
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
