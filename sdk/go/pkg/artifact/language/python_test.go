package language

import (
	"os"
	"reflect"
	"strings"
	"testing"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	artifactbuilder "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
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

func TestLanguageBuildersAcceptStringAndEnumSystems(t *testing.T) {
	stringSystems := []string{"aarch64-darwin", "x86_64-linux"}
	enumSystems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	stringPython := NewPython("python", stringSystems)
	enumPython := NewPython("python", enumSystems)
	if !reflect.DeepEqual(stringPython.systems, enumPython.systems) {
		t.Fatalf("NewPython string systems = %v, enum systems = %v", stringPython.systems, enumPython.systems)
	}

	builders := [][]api.ArtifactSystem{
		NewGo("go", stringSystems).systems,
		NewGo("go", enumSystems).systems,
		NewGoDevelopmentEnvironment("go-dev", stringSystems).systems,
		NewGoDevelopmentEnvironment("go-dev", enumSystems).systems,
		NewPythonDevelopmentEnvironment("python-dev", stringSystems).systems,
		NewPythonDevelopmentEnvironment("python-dev", enumSystems).systems,
		NewRust("rust", stringSystems).systems,
		NewRust("rust", enumSystems).systems,
		NewRustDevelopmentEnvironment("rust-dev", stringSystems).systems,
		NewRustDevelopmentEnvironment("rust-dev", enumSystems).systems,
		NewTypeScript("typescript", stringSystems).systems,
		NewTypeScript("typescript", enumSystems).systems,
		NewTypeScriptDevelopmentEnvironment("typescript-dev", stringSystems).systems,
		NewTypeScriptDevelopmentEnvironment("typescript-dev", enumSystems).systems,
	}

	for _, got := range builders {
		if !reflect.DeepEqual(got, enumSystems) {
			t.Fatalf("builder systems = %v, want %v", got, enumSystems)
		}
	}
}

func TestLanguageBuilderBuildReturnsSystemNormalizationError(t *testing.T) {
	_, err := NewPython("python", []string{"freebsd-riscv64"}).Build(nil)
	if err == nil {
		t.Fatal("Build returned nil error")
	}
	if err.Error() != "unsupported system: freebsd-riscv64" {
		t.Fatalf("Build error = %q", err.Error())
	}
}

func TestCoreBuildersAcceptStringAndEnumSystems(t *testing.T) {
	stringSystems := []string{"aarch64-darwin", "x86_64-linux"}
	enumSystems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	ctx := configContextWithTarget(t, "x86_64-darwin")
	step := artifactbuilder.NewArtifactStep("true").Build()
	tests := []struct {
		name        string
		buildString func() error
		buildEnum   func() error
	}{
		{
			name: "NewArtifact",
			buildString: func() error {
				_, err := artifactbuilder.NewArtifact("artifact", []*api.ArtifactStep{step}, stringSystems).Build(ctx)
				return err
			},
			buildEnum: func() error {
				_, err := artifactbuilder.NewArtifact("artifact", []*api.ArtifactStep{step}, enumSystems).Build(ctx)
				return err
			},
		},
		{
			name: "NewJob",
			buildString: func() error {
				_, err := artifactbuilder.NewJob("job", "true", stringSystems).Build(ctx)
				return err
			},
			buildEnum: func() error {
				_, err := artifactbuilder.NewJob("job", "true", enumSystems).Build(ctx)
				return err
			},
		},
		{
			name: "NewProcess",
			buildString: func() error {
				_, err := artifactbuilder.NewProcess("process", "true", stringSystems).Build(ctx)
				return err
			},
			buildEnum: func() error {
				_, err := artifactbuilder.NewProcess("process", "true", enumSystems).Build(ctx)
				return err
			},
		},
		{
			name: "NewDevelopmentEnvironment",
			buildString: func() error {
				_, err := artifactbuilder.NewDevelopmentEnvironment("devenv", stringSystems).Build(ctx)
				return err
			},
			buildEnum: func() error {
				_, err := artifactbuilder.NewDevelopmentEnvironment("devenv", enumSystems).Build(ctx)
				return err
			},
		},
		{
			name: "NewUserEnvironment",
			buildString: func() error {
				_, err := artifactbuilder.NewUserEnvironment("userenv", stringSystems).Build(ctx)
				return err
			},
			buildEnum: func() error {
				_, err := artifactbuilder.NewUserEnvironment("userenv", enumSystems).Build(ctx)
				return err
			},
		},
	}

	for _, test := range tests {
		stringErr := test.buildString()
		if stringErr == nil {
			t.Fatalf("%s string Build returned nil error", test.name)
		}

		enumErr := test.buildEnum()
		if enumErr == nil {
			t.Fatalf("%s enum Build returned nil error", test.name)
		}

		if stringErr.Error() != enumErr.Error() {
			t.Fatalf("%s string Build error = %q, enum Build error = %q", test.name, stringErr.Error(), enumErr.Error())
		}

		if !strings.Contains(stringErr.Error(), "supported: [AARCH64_DARWIN X8664_LINUX]") {
			t.Fatalf("%s Build error = %q, want normalized systems %v", test.name, stringErr.Error(), enumSystems)
		}
	}
}

func configContextWithTarget(t *testing.T, target string) *config.ConfigContext {
	t.Helper()
	t.Setenv("VORPAL_SOCKET_PATH", "")

	originalArgs := os.Args
	os.Args = []string{
		"vorpal",
		"start",
		"--artifact", "test",
		"--artifact-context", "test",
		"--artifact-namespace", "test",
		"--artifact-system", target,
		"--port", "1",
	}
	t.Cleanup(func() { os.Args = originalArgs })

	return config.GetContext()
}

func TestCoreBuilderBuildReturnsSystemNormalizationError(t *testing.T) {
	_, err := artifactbuilder.NewArtifact("artifact", nil, []string{"freebsd-riscv64"}).Build(nil)
	if err == nil {
		t.Fatal("Build returned nil error")
	}
	if err.Error() != "unsupported system: freebsd-riscv64" {
		t.Fatalf("Build error = %q", err.Error())
	}
}
