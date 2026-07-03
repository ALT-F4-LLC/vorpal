package language

import (
	"strings"
	"testing"
)

func TestStepBuildCommandAppMode(t *testing.T) {
	entrypoint := "src/example.py"
	script := stepBuildCommand("example", &entrypoint, "$VORPAL_ARTIFACT_PY/bin")

	// Launcher lands at the expected output path.
	if !strings.Contains(script, `"$VORPAL_OUTPUT/bin/example"`) {
		t.Error("expected launcher output path in script")
	}

	// Interpreter store path is baked (unescaped); runtime vars stay escaped so the
	// unquoted heredoc writes them as $ in the launcher without expanding them.
	if !strings.Contains(script, `exec "$VORPAL_ARTIFACT_PY/bin/python3" "\$VORPAL_PYTHON_ROOT/src/example.py" "\$@"`) {
		t.Error("expected argv-forwarding exec line with baked interpreter and escaped runtime vars")
	}

	// App mode does not build a wheel.
	if strings.Contains(script, "uv build") {
		t.Error("app mode must not emit uv build")
	}
}

func TestStepBuildCommandLibraryMode(t *testing.T) {
	script := stepBuildCommand("example", nil, "$VORPAL_ARTIFACT_PY/bin")

	if !strings.Contains(script, "uv build") {
		t.Error("expected uv build in library mode")
	}

	if !strings.Contains(script, `cp -pr dist/. "$VORPAL_OUTPUT/"`) {
		t.Error("expected dist copy")
	}

	if !strings.Contains(script, `cp uv.lock "$VORPAL_OUTPUT/"`) {
		t.Error("expected uv.lock copy")
	}

	// Library mode emits no launcher.
	if strings.Contains(script, "/bin/example") {
		t.Error("library mode must not emit a launcher")
	}
}
