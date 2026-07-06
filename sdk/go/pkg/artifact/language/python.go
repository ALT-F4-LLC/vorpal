package language

import (
	"fmt"
	"slices"
	"strings"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

// sourceDateEpoch is the zip epoch (1980-01-01T00:00:00Z) for reproducible uv build wheels.
// Wheels are zip archives; zip cannot represent dates before 1980, so SOURCE_DATE_EPOCH=0
// would yield an invalid wheel.
const sourceDateEpoch = "315532800"

type Python struct {
	aliases       []string
	artifacts     []*string
	entrypoint    *string
	environments  []string
	includes      []string
	name          string
	secrets       map[string]string
	sourceScripts []string
	systems       []api.ArtifactSystem
	workingDir    *string
}

// stepBuildCommand composes the mode-specific portion of the build step script.
//
// App mode (entrypoint set) emits a relocatable launcher at $VORPAL_OUTPUT/bin/<name>
// that forwards its argv ("$@") to the entrypoint. The interpreter store path (pythonBin)
// is baked at build time via %s — UNESCAPED — so the unquoted heredoc writes it literally
// into the launcher. All runtime vars ($VORPAL_PYTHON_ROOT, $@, ${BASH_SOURCE}) are
// escaped as \$ so the unquoted heredoc writes them as $ without expanding them during
// the build step. A quoted heredoc would suppress ALL substitution, including the baked
// interpreter path, so the heredoc MUST remain unquoted.
//
// Library mode (nil entrypoint) runs uv build and copies wheel, pyproject.toml, and
// uv.lock to $VORPAL_OUTPUT/.
func stepBuildCommand(name string, entrypoint *string, pythonBin string) string {
	if entrypoint != nil {
		return fmt.Sprintf(`cp -pr . "$VORPAL_OUTPUT/"

mkdir -p "$VORPAL_OUTPUT/bin"

cat > "$VORPAL_OUTPUT/bin/%s" << EOF
#!/usr/bin/env bash
set -euo pipefail
VORPAL_PYTHON_ROOT="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")/.." && pwd)"
PYTHONPATH_EXTRA="\$VORPAL_PYTHON_ROOT"
for site in "\$VORPAL_PYTHON_ROOT"/.venv/lib/python*/site-packages; do
    [ -d "\$site" ] && PYTHONPATH_EXTRA="\$site:\$PYTHONPATH_EXTRA"
done
export PYTHONPATH="\$PYTHONPATH_EXTRA\${PYTHONPATH:+:\$PYTHONPATH}"
exec "%s/python3" "\$VORPAL_PYTHON_ROOT/%s" "\$@"
EOF

chmod +x "$VORPAL_OUTPUT/bin/%s"`,
			name, pythonBin, *entrypoint, name)
	}
	return `uv build

mkdir -p "$VORPAL_OUTPUT"

cp -pr dist/. "$VORPAL_OUTPUT/"
cp pyproject.toml "$VORPAL_OUTPUT/"
cp uv.lock "$VORPAL_OUTPUT/"`
}

func NewPython(name string, systems []api.ArtifactSystem) *Python {
	return &Python{
		aliases:       []string{},
		artifacts:     []*string{},
		entrypoint:    nil,
		environments:  []string{},
		includes:      []string{},
		name:          name,
		secrets:       map[string]string{},
		sourceScripts: []string{},
		systems:       systems,
		workingDir:    nil,
	}
}

func (b *Python) WithAliases(aliases []string) *Python {
	b.aliases = aliases
	return b
}

func (b *Python) WithArtifacts(artifacts []*string) *Python {
	b.artifacts = artifacts
	return b
}

func (b *Python) WithEntrypoint(entrypoint string) *Python {
	b.entrypoint = &entrypoint
	return b
}

func (b *Python) WithEnvironments(environments []string) *Python {
	b.environments = environments
	return b
}

func (b *Python) WithIncludes(includes []string) *Python {
	b.includes = includes
	return b
}

func (b *Python) WithSecrets(secrets map[string]string) *Python {
	for k, v := range secrets {
		if _, exists := b.secrets[k]; !exists {
			b.secrets[k] = v
		}
	}
	return b
}

func (b *Python) WithSourceScripts(scripts []string) *Python {
	for _, script := range scripts {
		if !slices.Contains(b.sourceScripts, script) {
			b.sourceScripts = append(b.sourceScripts, script)
		}
	}
	return b
}

func (b *Python) WithWorkingDir(dir string) *Python {
	b.workingDir = &dir
	return b
}

func (b *Python) Build(context *config.ConfigContext) (*string, error) {
	// Resolve toolchain artifacts
	cpythonDigest, err := artifact.Cpython(context)
	if err != nil {
		return nil, err
	}

	cpythonBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*cpythonDigest))

	uvDigest, err := artifact.Uv(context)
	if err != nil {
		return nil, err
	}

	uvBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*uvDigest))

	// Build source
	sourcePath := "."

	sourceBuilder := artifact.NewArtifactSource(b.name, sourcePath)

	if len(b.includes) > 0 {
		sourceBuilder = sourceBuilder.WithIncludes(b.includes)
	}

	src := sourceBuilder.Build()
	source := &src

	// Setup step source directory
	stepSourceDir := fmt.Sprintf("%s/source/%s", sourcePath, source.Name)

	if b.workingDir != nil {
		stepSourceDir = fmt.Sprintf("%s/%s", stepSourceDir, *b.workingDir)
	}

	// Build step script
	//
	// uv sync --frozen is the hash-enforcement surface: uv verifies every package against
	// the per-package SHA-256 in uv.lock and fails on a content-hash mismatch. There is no
	// uv sync --require-hashes flag (--require-hashes is uv's pip-interface flag).
	// UV_PYTHON_DOWNLOADS=never + UV_PYTHON pinned to the Vorpal interpreter guarantee uv
	// never fetches an interpreter at build time.
	buildCmd := stepBuildCommand(b.name, b.entrypoint, cpythonBin)

	sourceScripts := strings.Join(b.sourceScripts, "\n")

	stepScript := fmt.Sprintf("pushd %s\n\n%s\n\nuv sync --frozen --no-dev --no-editable\n\n%s",
		stepSourceDir, sourceScripts, buildCmd)

	// Build environments
	environments := []string{
		fmt.Sprintf("PATH=%s:%s", uvBin, cpythonBin),
		fmt.Sprintf("UV_PYTHON=%s/python3", cpythonBin),
		"UV_PYTHON_DOWNLOADS=never",
		"UV_LINK_MODE=copy",
		"UV_CACHE_DIR=$VORPAL_WORKSPACE/uv/cache",
		fmt.Sprintf("SOURCE_DATE_EPOCH=%s", sourceDateEpoch),
	}

	for _, env := range b.environments {
		environments = append(environments, env)
	}

	// Build artifacts list (toolchain first, then caller-supplied)
	artifacts := []*string{cpythonDigest, uvDigest}
	artifacts = append(artifacts, b.artifacts...)

	step, err := artifact.Shell(context, artifacts, environments, stepScript, artifact.SecretsToProto(b.secrets))
	if err != nil {
		return nil, err
	}

	return artifact.NewArtifact(b.name, []*api.ArtifactStep{step}, b.systems).
		WithAliases(b.aliases).
		WithSources([]*api.ArtifactSource{source}).
		Build(context)
}

// ---------------------------------------------------------------------------
// Python Development Environment
// ---------------------------------------------------------------------------

type PythonDevelopmentEnvironment struct {
	artifacts    []*string
	environments []string
	name         string
	secrets      map[string]string
	systems      []api.ArtifactSystem
}

func NewPythonDevelopmentEnvironment(name string, systems []api.ArtifactSystem) *PythonDevelopmentEnvironment {
	return &PythonDevelopmentEnvironment{
		artifacts:    []*string{},
		environments: []string{},
		name:         name,
		secrets:      map[string]string{},
		systems:      systems,
	}
}

func (b *PythonDevelopmentEnvironment) WithArtifacts(artifacts []*string) *PythonDevelopmentEnvironment {
	b.artifacts = append(b.artifacts, artifacts...)
	return b
}

func (b *PythonDevelopmentEnvironment) WithEnvironments(environments []string) *PythonDevelopmentEnvironment {
	b.environments = append(b.environments, environments...)
	return b
}

func (b *PythonDevelopmentEnvironment) WithSecrets(secrets map[string]string) *PythonDevelopmentEnvironment {
	for k, v := range secrets {
		if _, exists := b.secrets[k]; !exists {
			b.secrets[k] = v
		}
	}
	return b
}

func (b *PythonDevelopmentEnvironment) Build(context *config.ConfigContext) (*string, error) {
	cpython, err := artifact.Cpython(context)
	if err != nil {
		return nil, err
	}

	cpythonBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*cpython))

	uv, err := artifact.Uv(context)
	if err != nil {
		return nil, err
	}

	// Pin the dev-shell interpreter and suppress uv's auto-download so the
	// shell always uses the Vorpal-managed CPython.
	environments := []string{
		fmt.Sprintf("UV_PYTHON=%s/python3", cpythonBin),
		"UV_PYTHON_DOWNLOADS=never",
	}
	environments = append(environments, b.environments...)

	artifacts := []*string{cpython, uv}
	artifacts = append(artifacts, b.artifacts...)

	devenv := artifact.NewDevelopmentEnvironment(b.name, b.systems).
		WithArtifacts(artifacts).
		WithEnvironments(environments)

	if len(b.secrets) > 0 {
		devenv = devenv.WithSecrets(b.secrets)
	}

	return devenv.Build(context)
}
