import { describe, expect, test } from "bun:test";
import type { Artifact } from "../api/artifact/artifact.js";
import { ArtifactSystem } from "../api/artifact/artifact.js";
import { computeArtifactDigest } from "../context.js";

// ---------------------------------------------------------------------------
// Simulated ConfigContext store for testing addArtifact/getArtifacts behavior
// ---------------------------------------------------------------------------

class TestStore {
  private artifacts = new Map<string, Artifact>();

  addArtifact(artifact: Artifact): string {
    const digest = computeArtifactDigest(artifact);
    if (!this.artifacts.has(digest)) {
      this.artifacts.set(digest, artifact);
    }
    return digest;
  }

  getArtifact(digest: string): Artifact | undefined {
    return this.artifacts.get(digest);
  }

  getArtifacts(): string[] {
    return Array.from(this.artifacts.keys()).sort();
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("context", () => {
  function makeArtifact(name: string, script: string): Artifact {
    return {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script,
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name,
    };
  }

  test("addArtifact returns consistent digest for same input", () => {
    const store = new TestStore();
    const artifact = makeArtifact("test", "echo hello");

    const digest1 = store.addArtifact(artifact);
    const digest2 = store.addArtifact(artifact);

    expect(digest1).toBe(digest2);
    expect(digest1).toMatch(/^[0-9a-f]{64}$/);
  });

  test("addArtifact returns cached digest on duplicate add", () => {
    const store = new TestStore();
    const artifact = makeArtifact("test", "echo hello");

    const digest1 = store.addArtifact(artifact);
    const digest2 = store.addArtifact(artifact);

    expect(digest1).toBe(digest2);
    // Store should only have one entry
    expect(store.getArtifacts()).toHaveLength(1);
  });

  test("different artifacts produce different digests", () => {
    const store = new TestStore();
    const artifact1 = makeArtifact("test1", "echo hello");
    const artifact2 = makeArtifact("test2", "echo world");

    const digest1 = store.addArtifact(artifact1);
    const digest2 = store.addArtifact(artifact2);

    expect(digest1).not.toBe(digest2);
  });

  test("getArtifacts returns sorted digests", () => {
    const store = new TestStore();
    const artifacts = [
      makeArtifact("z-artifact", "echo z"),
      makeArtifact("a-artifact", "echo a"),
      makeArtifact("m-artifact", "echo m"),
    ];

    const digests: string[] = [];
    for (const artifact of artifacts) {
      digests.push(store.addArtifact(artifact));
    }

    const sortedDigests = store.getArtifacts();
    expect(sortedDigests).toEqual([...digests].sort());
  });

  test("getArtifact retrieves stored artifact by digest", () => {
    const store = new TestStore();
    const artifact = makeArtifact("test", "echo hello");
    const digest = store.addArtifact(artifact);

    const retrieved = store.getArtifact(digest);
    expect(retrieved).toBeDefined();
    expect(retrieved!.name).toBe("test");
  });

  test("getArtifact returns undefined for unknown digest", () => {
    const store = new TestStore();
    expect(store.getArtifact("nonexistent")).toBeUndefined();
  });

  test("digest is hex-encoded SHA-256", () => {
    const store = new TestStore();
    const artifact = makeArtifact("test", "echo hello");
    const digest = store.addArtifact(artifact);

    // SHA-256 hex digest is 64 characters
    expect(digest).toHaveLength(64);
    expect(digest).toMatch(/^[0-9a-f]+$/);
  });

  test("artifact with different target produces different digest", () => {
    const store = new TestStore();
    const artifact1: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "echo test",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test",
    };

    const artifact2: Artifact = {
      ...artifact1,
      target: ArtifactSystem.X8664_DARWIN,
      systems: [ArtifactSystem.X8664_DARWIN],
    };

    const digest1 = store.addArtifact(artifact1);
    const digest2 = store.addArtifact(artifact2);

    expect(digest1).not.toBe(digest2);
  });
});
