import { describe, expect, test } from "bun:test";

// ---------------------------------------------------------------------------
// SDK exports validation
//
// Verify all expected exports from @vorpal/sdk are available for
// TypeScript config authors.
// ---------------------------------------------------------------------------

describe("SDK exports for TypeScript configs", () => {
  test("ConfigContext is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.ConfigContext).toBeDefined();
  });

  test("Job is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.Job).toBeDefined();
  });

  test("Process is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.Process).toBeDefined();
  });

  test("DevelopmentEnvironment is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.DevelopmentEnvironment).toBeDefined();
  });

  test("GoDevelopmentEnvironment is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.GoDevelopmentEnvironment).toBeDefined();
  });

  test("RustDevelopmentEnvironment is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.RustDevelopmentEnvironment).toBeDefined();
  });

  test("TypeScriptDevelopmentEnvironment is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.TypeScriptDevelopmentEnvironment).toBeDefined();
  });

  test("UserEnvironment is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.UserEnvironment).toBeDefined();
  });

  test("Artifact is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.Artifact).toBeDefined();
  });

  test("ArtifactSource is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.ArtifactSource).toBeDefined();
  });

  test("ArtifactStep is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.ArtifactStep).toBeDefined();
  });

  test("ArtifactSystem enum is exported", async () => {
    const mod = await import("../index.js");
    expect(mod.ArtifactSystem).toBeDefined();
    expect(mod.ArtifactSystem.AARCH64_DARWIN).toBeDefined();
    expect(mod.ArtifactSystem.AARCH64_LINUX).toBeDefined();
    expect(mod.ArtifactSystem.X8664_DARWIN).toBeDefined();
    expect(mod.ArtifactSystem.X8664_LINUX).toBeDefined();
  });

  test("step functions are exported", async () => {
    const mod = await import("../index.js");
    expect(typeof mod.bash).toBe("function");
    expect(typeof mod.bwrap).toBe("function");
    expect(typeof mod.shell).toBe("function");
    expect(typeof mod.docker).toBe("function");
  });

  test("system utilities are exported", async () => {
    const mod = await import("../index.js");
    expect(typeof mod.getSystem).toBe("function");
    expect(typeof mod.getSystemDefault).toBe("function");
    expect(typeof mod.getSystemStr).toBe("function");
    expect(typeof mod.getSystemDefaultStr).toBe("function");
  });

  test("getEnvKey is exported", async () => {
    const mod = await import("../index.js");
    expect(typeof mod.getEnvKey).toBe("function");
  });

  test("parseCliArgs is exported", async () => {
    const mod = await import("../index.js");
    expect(typeof mod.parseCliArgs).toBe("function");
  });

  test("alias functions are exported", async () => {
    const mod = await import("../index.js");
    expect(typeof mod.parseArtifactAlias).toBe("function");
    expect(typeof mod.formatArtifactAlias).toBe("function");
  });
});
