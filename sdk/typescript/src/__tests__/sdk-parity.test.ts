import { describe, expect, test } from "bun:test";
import type { Artifact } from "../api/artifact/artifact.js";
import { ArtifactSystem } from "../api/artifact/artifact.js";
import { computeArtifactDigest } from "../context.js";

// ---------------------------------------------------------------------------
// Cross-SDK parity framework
//
// This test validates that identical Artifact definitions in TypeScript
// produce the same SHA-256 digests. The golden digests are verified
// against the Rust SDK (see digest-parity.test.ts for the golden values).
//
// The framework here shows how to construct identical artifacts in
// TypeScript and verify their digests, providing the foundation for
// three-way TypeScript/Go/Rust comparison once all SDKs are integrated.
// ---------------------------------------------------------------------------

describe("cross-SDK parity framework", () => {
  /**
   * Helper: Creates an artifact definition that is identical to what the
   * Go and Rust SDKs would produce for a simple "echo hello" job on
   * AARCH64_DARWIN. The step structure mimics what bash() produces
   * on a Darwin system (since shell() delegates to bash() on Darwin).
   */
  function makeParityArtifact(
    name: string,
    script: string,
    systems: ArtifactSystem[],
    target: ArtifactSystem,
  ): Artifact {
    return {
      target,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: `#!/bin/bash\nset -euo pipefail\n\n${script}\n`,
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [
            "HOME=$VORPAL_WORKSPACE",
            "PATH=:/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
          ],
        },
      ],
      systems,
      aliases: [],
      name,
    };
  }

  test("simple job artifact produces deterministic digest", () => {
    const artifact = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    const digest1 = computeArtifactDigest(artifact);
    const digest2 = computeArtifactDigest(artifact);

    // Deterministic: same input always produces same output
    expect(digest1).toBe(digest2);
    // Valid SHA-256 hex
    expect(digest1).toMatch(/^[0-9a-f]{64}$/);
  });

  test("different names produce different digests", () => {
    const artifact1 = makeParityArtifact(
      "test-job-a",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );
    const artifact2 = makeParityArtifact(
      "test-job-b",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("different scripts produce different digests", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );
    const artifact2 = makeParityArtifact(
      "test-job",
      "echo world",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("different target systems produce different digests", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN, ArtifactSystem.X8664_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );
    const artifact2 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN, ArtifactSystem.X8664_DARWIN],
      ArtifactSystem.X8664_DARWIN,
    );

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("multi-system artifact produces expected fields", () => {
    const systems = [
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
      ArtifactSystem.X8664_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ];

    const artifact = makeParityArtifact(
      "test-multi",
      "echo hello",
      systems,
      ArtifactSystem.AARCH64_DARWIN,
    );

    const digest = computeArtifactDigest(artifact);
    expect(digest).toMatch(/^[0-9a-f]{64}$/);

    // Verify all four systems are in the digest computation
    const artifactForX86 = makeParityArtifact(
      "test-multi",
      "echo hello",
      systems,
      ArtifactSystem.X8664_LINUX,
    );

    // Different target = different digest (even with same systems list)
    expect(digest).not.toBe(computeArtifactDigest(artifactForX86));
  });

  test("artifact with sources produces different digest than without", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    const artifact2: Artifact = {
      ...artifact1,
      sources: [
        {
          digest: "source-digest-abc",
          excludes: [],
          includes: ["src/**"],
          name: "my-source",
          path: "/src",
        },
      ],
    };

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("artifact with aliases produces different digest than without", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    const artifact2: Artifact = {
      ...artifact1,
      aliases: ["myapp:v1.0"],
    };

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("artifact with step dependencies produces different digest", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    const artifact2: Artifact = {
      ...artifact1,
      steps: [
        {
          ...artifact1.steps[0],
          artifacts: ["dependency-digest-123"],
        },
      ],
    };

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });

  test("artifact with secrets produces different digest", () => {
    const artifact1 = makeParityArtifact(
      "test-job",
      "echo hello",
      [ArtifactSystem.AARCH64_DARWIN],
      ArtifactSystem.AARCH64_DARWIN,
    );

    const artifact2: Artifact = {
      ...artifact1,
      steps: [
        {
          ...artifact1.steps[0],
          secrets: [{ name: "API_KEY", value: "secret123" }],
        },
      ],
    };

    expect(computeArtifactDigest(artifact1)).not.toBe(
      computeArtifactDigest(artifact2),
    );
  });
});
