import { describe, expect, test } from "bun:test";
import { stepBuildCommand } from "../../artifact/language/python.js";

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
