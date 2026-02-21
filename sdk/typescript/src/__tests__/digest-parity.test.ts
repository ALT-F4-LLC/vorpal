import { describe, expect, test } from "bun:test";
import type {
  Artifact,
  ArtifactStep,
  ArtifactSource,
} from "../api/artifact/artifact.js";
import { ArtifactSystem } from "../api/artifact/artifact.js";
import {
  artifactToJsonBytes,
  computeArtifactDigest,
  serializeArtifactStep,
  serializeArtifactSource,
} from "../context.js";

// ---------------------------------------------------------------------------
// Golden test vectors
//
// These JSON strings and SHA-256 digests represent the expected output from
// Rust serde_json::to_vec + sha256::digest for identical Artifact messages.
// The TypeScript custom serializer must produce byte-for-byte identical output.
// ---------------------------------------------------------------------------

describe("digest parity", () => {
  test("(a) minimal artifact", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho hello\n",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [
            "HOME=$VORPAL_WORKSPACE",
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
          ],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test-minimal",
    };

    const expectedJson =
      '{"target":1,"sources":[],"steps":[{"entrypoint":"bash","script":"#!/bin/bash\\nset -euo pipefail\\n\\necho hello\\n","secrets":[],"arguments":[],"artifacts":[],"environments":["HOME=$VORPAL_WORKSPACE","PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin"]}],"systems":[1],"aliases":[],"name":"test-minimal"}';
    const expectedDigest =
      "3d2025fad0c337457edd35f7eb04a4f507acb0610ad3818faa19ebcb81bd8f4c";

    const jsonBytes = artifactToJsonBytes(artifact);
    expect(jsonBytes.toString("utf-8")).toBe(expectedJson);
    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(b) full artifact with all fields populated", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [
        {
          digest: "src-digest-1",
          excludes: ["*.log"],
          includes: ["**/*.ts"],
          name: "source-a",
          path: "/src/a",
        },
        {
          digest: undefined,
          excludes: [],
          includes: [],
          name: "source-b",
          path: "/src/b",
        },
      ],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho step1\n",
          secrets: [{ name: "SECRET_A", value: "val-a" }],
          arguments: [],
          artifacts: ["dep1"],
          environments: [
            "HOME=$VORPAL_WORKSPACE",
            "PATH=/usr/local/bin:/usr/bin:/usr/sbin:/bin:/sbin",
          ],
        },
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho step2\n",
          secrets: [],
          arguments: ["--flag"],
          artifacts: [],
          environments: ["HOME=$VORPAL_WORKSPACE"],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN, ArtifactSystem.X8664_DARWIN],
      aliases: ["mylib:latest", "mylib:v1.0"],
      name: "test-full",
    };

    const expectedDigest =
      "dffc2ae5193ca47da65fef0bedc8e71c891da437d166f330c7a8908fd61853bd";

    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(c) artifact with empty repeated fields", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho test\n",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test-empty-repeated",
    };

    const expectedJson =
      '{"target":1,"sources":[],"steps":[{"entrypoint":"bash","script":"#!/bin/bash\\nset -euo pipefail\\n\\necho test\\n","secrets":[],"arguments":[],"artifacts":[],"environments":[]}],"systems":[1],"aliases":[],"name":"test-empty-repeated"}';
    const expectedDigest =
      "b09faf657bab9975ca9d846eab1f4618b33a90fa2f760d1d16dc892dc9089287";

    const jsonBytes = artifactToJsonBytes(artifact);
    expect(jsonBytes.toString("utf-8")).toBe(expectedJson);
    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(d) artifact with optional fields present and absent", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.X8664_DARWIN,
      sources: [
        {
          digest: "abc123",
          excludes: ["*.tmp"],
          includes: ["src/**"],
          name: "source1",
          path: "/path/to/source",
        },
      ],
      steps: [
        {
          entrypoint: undefined,
          script: undefined,
          secrets: [],
          arguments: ["--foo"],
          artifacts: ["dep1"],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.X8664_DARWIN],
      aliases: [],
      name: "test-optional-fields",
    };

    const expectedJson =
      '{"target":3,"sources":[{"digest":"abc123","excludes":["*.tmp"],"includes":["src/**"],"name":"source1","path":"/path/to/source"}],"steps":[{"entrypoint":null,"script":null,"secrets":[],"arguments":["--foo"],"artifacts":["dep1"],"environments":[]}],"systems":[3],"aliases":[],"name":"test-optional-fields"}';
    const expectedDigest =
      "0dc78aa89f11cbf532d1b6166acfcb5344ee5d56cd30357883862778f2533427";

    const jsonBytes = artifactToJsonBytes(artifact);
    expect(jsonBytes.toString("utf-8")).toBe(expectedJson);
    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(e) artifact with zero-value enum (UNKNOWN_SYSTEM = 0)", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.UNKNOWN_SYSTEM,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho test\n",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.UNKNOWN_SYSTEM],
      aliases: [],
      name: "test-zero-enum",
    };

    const expectedJson =
      '{"target":0,"sources":[],"steps":[{"entrypoint":"bash","script":"#!/bin/bash\\nset -euo pipefail\\n\\necho test\\n","secrets":[],"arguments":[],"artifacts":[],"environments":[]}],"systems":[0],"aliases":[],"name":"test-zero-enum"}';
    const expectedDigest =
      "d55848939e5204a8a905ff56b3ef7429f7714d129df32922330866bffd9cf53a";

    const jsonBytes = artifactToJsonBytes(artifact);
    expect(jsonBytes.toString("utf-8")).toBe(expectedJson);
    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(f) complex step with all fields populated", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\nmake build\n",
          secrets: [
            { name: "API_KEY", value: "secret123" },
            { name: "DB_PASS", value: "dbsecret" },
          ],
          arguments: ["--verbose", "--output=/tmp"],
          artifacts: ["dep1", "dep2"],
          environments: ["HOME=/root", "CC=gcc"],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN, ArtifactSystem.AARCH64_LINUX],
      aliases: ["myapp:v1.0"],
      name: "test-complex",
    };

    const expectedDigest =
      "1a69b2a2ab53eec76fd64c55e50ccb5ad22759372feb2e350bd980304569d2b4";

    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("(g) step with sorted secrets", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho sorted\n",
          secrets: [
            { name: "ALPHA", value: "a" },
            { name: "BETA", value: "b" },
            { name: "GAMMA", value: "g" },
          ],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test-sorted-secrets",
    };

    const expectedDigest =
      "9ca5effbb88c322d4d5ab1d96856b515e5124a2976ef19793dbed229683c56d7";

    expect(computeArtifactDigest(artifact)).toBe(expectedDigest);
  });

  test("field order matches proto field number order", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "#!/bin/bash\nset -euo pipefail\n\necho hello\n",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test-field-order",
    };

    const json = artifactToJsonBytes(artifact).toString("utf-8");
    const parsed = JSON.parse(json);
    const keys = Object.keys(parsed);

    // Proto field order: target(1), sources(2), steps(3), systems(4), aliases(5), name(6)
    expect(keys).toEqual([
      "target",
      "sources",
      "steps",
      "systems",
      "aliases",
      "name",
    ]);
  });

  test("step field order matches proto field number order", () => {
    const step: ArtifactStep = {
      entrypoint: "bash",
      script: "test",
      secrets: [],
      arguments: [],
      artifacts: [],
      environments: [],
    };

    const serialized = serializeArtifactStep(step);
    const json = JSON.stringify(serialized);
    const parsed = JSON.parse(json);
    const keys = Object.keys(parsed);

    // Proto field order: entrypoint(1), script(2), secrets(3), arguments(4), artifacts(5), environments(6)
    expect(keys).toEqual([
      "entrypoint",
      "script",
      "secrets",
      "arguments",
      "artifacts",
      "environments",
    ]);
  });

  test("source field order matches proto field number order", () => {
    const source: ArtifactSource = {
      digest: "abc",
      excludes: [],
      includes: [],
      name: "test",
      path: "/test",
    };

    const serialized = serializeArtifactSource(source);
    const json = JSON.stringify(serialized);
    const parsed = JSON.parse(json);
    const keys = Object.keys(parsed);

    // Proto field order: digest(1), excludes(2), includes(3), name(4), path(5)
    expect(keys).toEqual(["digest", "excludes", "includes", "name", "path"]);
  });

  test("enums serialize as integers not strings", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.X8664_LINUX,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "test",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [
        ArtifactSystem.AARCH64_DARWIN,
        ArtifactSystem.X8664_LINUX,
      ],
      aliases: [],
      name: "test-enum-int",
    };

    const json = artifactToJsonBytes(artifact).toString("utf-8");
    // target should be integer 4 (X8664_LINUX), not "X8664_LINUX"
    expect(json).toContain('"target":4');
    expect(json).not.toContain('"target":"X8664_LINUX"');
    // systems should be [1,4]
    expect(json).toContain('"systems":[1,4]');
  });

  test("optional None serializes as null", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [
        {
          digest: undefined,
          excludes: [],
          includes: [],
          name: "test",
          path: "/test",
        },
      ],
      steps: [
        {
          entrypoint: undefined,
          script: undefined,
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "test-null",
    };

    const json = artifactToJsonBytes(artifact).toString("utf-8");
    expect(json).toContain('"digest":null');
    expect(json).toContain('"entrypoint":null');
    expect(json).toContain('"script":null');
  });

  test("same artifact always produces same digest (deterministic)", () => {
    const artifact: Artifact = {
      target: ArtifactSystem.AARCH64_DARWIN,
      sources: [],
      steps: [
        {
          entrypoint: "bash",
          script: "echo hello",
          secrets: [],
          arguments: [],
          artifacts: [],
          environments: [],
        },
      ],
      systems: [ArtifactSystem.AARCH64_DARWIN],
      aliases: [],
      name: "deterministic-test",
    };

    const digest1 = computeArtifactDigest(artifact);
    const digest2 = computeArtifactDigest(artifact);
    const digest3 = computeArtifactDigest(artifact);

    expect(digest1).toBe(digest2);
    expect(digest2).toBe(digest3);
  });
});
