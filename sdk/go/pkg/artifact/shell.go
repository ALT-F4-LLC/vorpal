package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type ScriptDevenvTemplateArgs struct {
	Backups  string
	Exports  string
	Restores string
	Unsets   string
}

const ScriptDevenvTemplate = `
mkdir -pv $VORPAL_WORKSPACE/bin

cat > bin/activate << "EOF"
#!/bin/bash

{{.Backups}}
{{.Exports}}

deactivate(){
{{.Restores}}
{{.Unsets}}
}

exec "$@"
EOF

chmod +x $VORPAL_WORKSPACE/bin/activate

mkdir -pv $VORPAL_OUTPUT/bin

cp -prv bin "$VORPAL_OUTPUT"`

type ScriptUserenvTemplateArgs struct {
	Path               string
	SymlinksActivate   string
	SymlinksDeactivate string
}

const ScriptUserenvTemplate = `
mkdir -pv $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/activate-shell << "EOF"
#!/bin/bash
export PATH="$VORPAL_OUTPUT/bin:{{.Path}}:$PATH"
EOF

cat > $VORPAL_OUTPUT/bin/activate-symlinks << "EOF"
#!/bin/bash

if [ -x "$(command -v deactivate-symlinks)" ]; then
    deactivate-symlinks
fi

echo "Activating new symlinks..."

{{.SymlinksActivate}}
EOF

cat > $VORPAL_OUTPUT/bin/deactivate-symlinks << "EOF"
#!/bin/bash

echo "Deactivating existing symlinks..."

{{.SymlinksDeactivate}}
EOF

chmod +x $VORPAL_OUTPUT/bin/activate-shell
chmod +x $VORPAL_OUTPUT/bin/activate-symlinks
chmod +x $VORPAL_OUTPUT/bin/deactivate-symlinks`

func ScriptDevenv(
	context *config.ConfigContext,
	artifacts []*string,
	environments []string,
	name string,
	secrets []*api.ArtifactStepSecret,
	systems []api.ArtifactSystem,
) (*string, error) {
	backups := []string{
		"export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"",
		"export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"",
		"export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"",
	}

	exports := []string{
		fmt.Sprintf("export PS1=\"(%s) $PS1\"", name),
		"export VORPAL_SHELL=\"1\"",
	}

	restores := []string{
		"export PATH=\"$VORPAL_SHELL_BACKUP_PATH\"",
		"export PS1=\"$VORPAL_SHELL_BACKUP_PS1\"",
		"export VORPAL_SHELL=\"$VORPAL_SHELL_BACKUP_VORPAL_SHELL\"",
	}

	unsets := []string{
		"unset VORPAL_SHELL_BACKUP_PATH",
		"unset VORPAL_SHELL_BACKUP_PS1",
		"unset VORPAL_SHELL_BACKUP_VORPAL_SHELL",
	}

	for _, envvar := range environments {
		key := strings.Split(envvar, "=")[0]

		if strings.Contains(envvar, "PATH=") {
			continue
		}

		backups = append(backups, fmt.Sprintf("export VORPAL_SHELL_BACKUP_%s=\"$%s\"", key, key))
		exports = append(exports, fmt.Sprintf("export %s", envvar))
		restores = append(restores, fmt.Sprintf("export %s=\"$VORPAL_SHELL_BACKUP_%s\"", key, key))
		unsets = append(unsets, fmt.Sprintf("unset VORPAL_SHELL_BACKUP_%s", key))
	}

	// Setup path

	stepPathArtifacts := make([]string, 0)

	for _, artifact := range artifacts {
		stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", GetEnvKey(artifact)))
	}

	stepPath := strings.Join(stepPathArtifacts, ":")

	for _, envvar := range environments {
		if strings.Contains(envvar, "PATH=") {
			stepPath = fmt.Sprintf("%s:%s", strings.Replace(envvar, "PATH=", "", 1), stepPath)
		}
	}

	exports = append(exports, fmt.Sprintf("export PATH=%s:$PATH", stepPath))

	// Setup script

	scriptTemplate, err := template.New("script").Parse(ScriptDevenvTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	stepScriptVars := ScriptDevenvTemplateArgs{
		Backups:  strings.Join(backups, "\n"),
		Exports:  strings.Join(exports, "\n"),
		Restores: strings.Join(restores, "\n"),
		Unsets:   strings.Join(unsets, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, stepScriptVars); err != nil {
		return nil, err
	}

	stepScript := scriptBuffer.String()

	step, err := Shell(context, artifacts, []string{}, stepScript, secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	artifact := NewArtifactBuilder(name, steps, systems)

	return artifact.Build(context)
}

func ScriptUserenv(
	context *config.ConfigContext,
	artifacts []*string,
	environments []string,
	name string,
	symlinks map[string]string,
	systems []api.ArtifactSystem,
) (*string, error) {
	// Setup path

	stepPathArtifacts := make([]string, 0)

	for _, artifact := range artifacts {
		stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", GetEnvKey(artifact)))
	}

	stepPath := strings.Join(stepPathArtifacts, ":")

	for _, envvar := range environments {
		if strings.Contains(envvar, "PATH=") {
			stepPath = fmt.Sprintf("%s:%s", strings.Replace(envvar, "PATH=", "", 1), stepPath)
		}
	}

	// Setup script

	scriptTemplate, err := template.New("script").Parse(ScriptUserenvTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	symlinksActivate := make([]string, 0)
	symlinksDeactivate := make([]string, 0)

	for source, target := range symlinks {
		symlinksActivate = append(symlinksActivate, fmt.Sprintf("ln -sfv %s %s", source, target))
		symlinksDeactivate = append(symlinksDeactivate, fmt.Sprintf("rm -fv %s", target))
	}

	stepScriptVars := ScriptUserenvTemplateArgs{
		Path:               stepPath,
		SymlinksActivate:   strings.Join(symlinksActivate, "\n"),
		SymlinksDeactivate: strings.Join(symlinksDeactivate, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, stepScriptVars); err != nil {
		return nil, err
	}

	stepScript := scriptBuffer.String()

	step, err := Shell(context, artifacts, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	artifact := NewArtifactBuilder(name, steps, systems)

	return artifact.Build(context)
}
