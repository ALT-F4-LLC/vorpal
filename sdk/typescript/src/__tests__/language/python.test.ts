import { describe, expect, test } from "bun:test";
import { stepBuildCommand } from "../../artifact/language/python.js";
import { Python } from "../../artifact/language/python.js";
import { ArtifactSystem } from "../../api/artifact/artifact.js";
import type { ConfigContext } from "../../context.js";

describe("stepBuildCommand", () => {
  test("app mode emits argv-forwarding launcher", () => {
    const script = stepBuildCommand(
      "example",
      "src/example.py",
      "$VORPAL_ARTIFACT_PY/bin",
    );

    // Launcher lands at the expected output path.
    expect(script).toContain('"$VORPAL_OUTPUT/bin/example"');

    // Interpreter store path is baked at build time (unescaped JS interpolation);
    // runtime vars stay backslash-escaped so the heredoc writes them literally.
    expect(script).toContain(
      'exec "$VORPAL_ARTIFACT_PY/bin/python3" "\\$VORPAL_PYTHON_ROOT/src/example.py" "\\$@"',
    );

    // App mode does not build a wheel.
    expect(script).not.toContain("uv build");
  });

  test("library mode emits wheel and lock", () => {
    const script = stepBuildCommand("example", undefined, "$VORPAL_ARTIFACT_PY/bin");

    expect(script).toContain("uv build");
    expect(script).toContain('cp -pr dist/. "$VORPAL_OUTPUT/"');
    expect(script).toContain('cp uv.lock "$VORPAL_OUTPUT/"');

    // Library mode emits no launcher.
    expect(script).not.toContain("/bin/example");
  });
});

describe("Python system inputs", () => {
  test("accepts raw canonical systems in the public constructor", () => {
    const builder = new Python("example", [
      "aarch64-darwin",
      "x86_64-linux",
    ]);

    expect(
      (builder as unknown as { _systems: ArtifactSystem[] })._systems,
    ).toEqual([
      ArtifactSystem.AARCH64_DARWIN,
      ArtifactSystem.X8664_LINUX,
    ]);
  });

  test("raises unsupported raw system errors from build", async () => {
    const builder = new Python("example", ["riscv64-linux"]);

    let error: Error | undefined;
    try {
      await builder.build({} as ConfigContext);
    } catch (caught) {
      error = caught instanceof Error ? caught : new Error(String(caught));
    }

    expect(error?.message).toBe("unsupported system: riscv64-linux");
  });
});
