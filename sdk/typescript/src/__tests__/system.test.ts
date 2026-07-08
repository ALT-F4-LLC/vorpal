import { describe, expect, test } from "bun:test";
import {
  Artifact,
  ArtifactStep,
  ArtifactSystem,
  normalizeSystems,
} from "../index.js";
import type { ArtifactSystemInput } from "../index.js";
import type { Artifact as ArtifactMsg } from "../api/artifact/artifact.js";
import type { ConfigContext } from "../context.js";

function captureArtifactContext(artifacts: ArtifactMsg[]): ConfigContext {
  return {
    getSystem: () => ArtifactSystem.X8664_LINUX,
    addArtifact: async (artifact: ArtifactMsg) => {
      artifacts.push(artifact);
      return "digest";
    },
  } as unknown as ConfigContext;
}

describe("normalizeSystems", () => {
  test("accepts canonical strings and enum values while preserving order", () => {
    const systems: ArtifactSystemInput[] = [
      "x86_64-linux",
      ArtifactSystem.AARCH64_DARWIN,
      "aarch64-linux",
    ];

    expect(normalizeSystems(systems)).toEqual([
      ArtifactSystem.X8664_LINUX,
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.AARCH64_LINUX,
    ]);
  });

  test("rejects unsupported strings", () => {
    expect(() => normalizeSystems(["riscv64-linux"])).toThrow(
      "unsupported system: riscv64-linux",
    );
  });

  test("rejects UNKNOWN_SYSTEM", () => {
    expect(() => normalizeSystems([ArtifactSystem.UNKNOWN_SYSTEM])).toThrow(
      "unsupported system: UNKNOWN_SYSTEM",
    );
  });
});

describe("Artifact system inputs", () => {
  test("serializes normalized protobuf enum systems", async () => {
    const artifacts: ArtifactMsg[] = [];
    const step = new ArtifactStep("bash").build();

    const digest = await new Artifact(
      "example",
      [step],
      ["x86_64-linux", ArtifactSystem.AARCH64_DARWIN],
    ).build(captureArtifactContext(artifacts));

    expect(digest).toBe("digest");
    expect(artifacts[0]?.systems).toEqual([
      ArtifactSystem.X8664_LINUX,
      ArtifactSystem.AARCH64_DARWIN,
    ]);
  });

  test("raises constructor normalization failures from build", async () => {
    const step = new ArtifactStep("bash").build();
    const artifact = new Artifact("example", [step], ["riscv64-linux"]);

    let error: Error | undefined;
    try {
      await artifact.build({} as ConfigContext);
    } catch (caught) {
      error = caught instanceof Error ? caught : new Error(String(caught));
    }

    expect(error?.message).toBe("unsupported system: riscv64-linux");
  });
});
