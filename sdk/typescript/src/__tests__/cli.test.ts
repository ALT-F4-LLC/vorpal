import { describe, expect, test, beforeEach, afterEach } from "bun:test";
import { parseCliArgs } from "../cli.js";

describe("CLI argument parsing", () => {
  let origEnv: string | undefined;

  beforeEach(() => {
    origEnv = process.env["VORPAL_SOCKET_PATH"];
    delete process.env["VORPAL_SOCKET_PATH"];
  });

  afterEach(() => {
    if (origEnv !== undefined) {
      process.env["VORPAL_SOCKET_PATH"] = origEnv;
    } else {
      delete process.env["VORPAL_SOCKET_PATH"];
    }
  });

  test("parses all required arguments", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "my-artifact",
      "--artifact-context",
      "/path/to/context",
      "--artifact-namespace",
      "my-ns",
      "--artifact-system",
      "aarch64-darwin",
      "--port",
      "8080",
    ]);

    expect(result.artifact).toBe("my-artifact");
    expect(result.artifactContext).toBe("/path/to/context");
    expect(result.artifactNamespace).toBe("my-ns");
    expect(result.artifactSystem).toBe("aarch64-darwin");
    expect(result.port).toBe(8080);
  });

  test("--agent sets agent address", () => {
    const result = parseCliArgs([
      "start",
      "--agent",
      "unix:///tmp/agent.sock",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-linux",
      "--port",
      "9090",
    ]);

    expect(result.agent).toBe("unix:///tmp/agent.sock");
  });

  test("--registry sets registry address", () => {
    const result = parseCliArgs([
      "start",
      "--registry",
      "https://registry.example.com",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-linux",
      "--port",
      "9090",
    ]);

    expect(result.registry).toBe("https://registry.example.com");
  });

  test("--artifact-unlock flag", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "aarch64-linux",
      "--artifact-unlock",
      "--port",
      "1234",
    ]);

    expect(result.artifactUnlock).toBe(true);
  });

  test("--artifact-unlock defaults to false", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "aarch64-linux",
      "--port",
      "1234",
    ]);

    expect(result.artifactUnlock).toBe(false);
  });

  test("--artifact-variable key=value pairs", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-darwin",
      "--port",
      "5000",
      "--artifact-variable",
      "KEY1=value1",
      "--artifact-variable",
      "KEY2=value2",
    ]);

    expect(result.artifactVariable).toEqual(["KEY1=value1", "KEY2=value2"]);
  });

  test("--artifact-variable with equals in value", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-darwin",
      "--port",
      "5000",
      "--artifact-variable",
      "OPTS=--flag=val",
    ]);

    expect(result.artifactVariable).toEqual(["OPTS=--flag=val"]);
  });

  test("empty artifact-variable list by default", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-darwin",
      "--port",
      "5000",
    ]);

    expect(result.artifactVariable).toEqual([]);
  });

  test("default agent and registry use socket path", () => {
    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-darwin",
      "--port",
      "5000",
    ]);

    expect(result.agent).toBe("unix:///var/lib/vorpal/vorpal.sock");
    expect(result.registry).toBe("unix:///var/lib/vorpal/vorpal.sock");
  });

  test("VORPAL_SOCKET_PATH env var overrides default", () => {
    process.env["VORPAL_SOCKET_PATH"] = "/tmp/custom.sock";

    const result = parseCliArgs([
      "start",
      "--artifact",
      "test",
      "--artifact-context",
      "/ctx",
      "--artifact-namespace",
      "ns",
      "--artifact-system",
      "x86_64-darwin",
      "--port",
      "5000",
    ]);

    expect(result.agent).toBe("unix:///tmp/custom.sock");
    expect(result.registry).toBe("unix:///tmp/custom.sock");
  });

  // -----------------------------------------------------------------
  // Error cases
  // -----------------------------------------------------------------

  test("throws on missing start subcommand", () => {
    expect(() => parseCliArgs([])).toThrow("expected 'start' subcommand");
  });

  test("throws on wrong subcommand", () => {
    expect(() => parseCliArgs(["run"])).toThrow("expected 'start' subcommand");
  });

  test("throws on missing --artifact", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact-context",
        "/ctx",
        "--artifact-namespace",
        "ns",
        "--artifact-system",
        "x86_64-darwin",
        "--port",
        "5000",
      ]),
    ).toThrow("--artifact is required");
  });

  test("throws on missing --artifact-context", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-namespace",
        "ns",
        "--artifact-system",
        "x86_64-darwin",
        "--port",
        "5000",
      ]),
    ).toThrow("--artifact-context is required");
  });

  test("throws on missing --artifact-namespace", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-context",
        "/ctx",
        "--artifact-system",
        "x86_64-darwin",
        "--port",
        "5000",
      ]),
    ).toThrow("--artifact-namespace is required");
  });

  test("throws on missing --artifact-system", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-context",
        "/ctx",
        "--artifact-namespace",
        "ns",
        "--port",
        "5000",
      ]),
    ).toThrow("--artifact-system is required");
  });

  test("throws on missing --port", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-context",
        "/ctx",
        "--artifact-namespace",
        "ns",
        "--artifact-system",
        "x86_64-darwin",
      ]),
    ).toThrow("--port is required");
  });

  test("throws on unknown argument", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-context",
        "/ctx",
        "--artifact-namespace",
        "ns",
        "--artifact-system",
        "x86_64-darwin",
        "--port",
        "5000",
        "--unknown-flag",
      ]),
    ).toThrow("unknown argument: --unknown-flag");
  });

  test("throws on flag missing value at end of args", () => {
    expect(() =>
      parseCliArgs(["start", "--artifact"]),
    ).toThrow("--artifact requires a value");
  });

  test("throws on --port missing value at end of args", () => {
    expect(() =>
      parseCliArgs([
        "start",
        "--artifact",
        "test",
        "--artifact-context",
        "/ctx",
        "--artifact-namespace",
        "ns",
        "--artifact-system",
        "x86_64-darwin",
        "--port",
      ]),
    ).toThrow("--port requires a value");
  });
});
