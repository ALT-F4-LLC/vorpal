package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type ShellArtifactTemplate struct {
	Backups  string
	Exports  string
	Restores string
	Unsets   string
}

const ShellArtifactScriptTemplate = `
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

func ScriptDevshell(context *config.ConfigContext, artifacts []*string, environments []string, name string) (*string, error) {
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

	scriptTemplate, err := template.New("script").Parse(ShellArtifactScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	stepScriptVars := ShellArtifactTemplate{
		Backups:  strings.Join(backups, "\n"),
		Exports:  strings.Join(exports, "\n"),
		Restores: strings.Join(restores, "\n"),
		Unsets:   strings.Join(unsets, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, stepScriptVars); err != nil {
		return nil, err
	}

	stepScript := scriptBuffer.String()

	step, err := Shell(context, artifacts, []string{}, stepScript)
	if err != nil {
		return nil, err
	}

	artifact := NewArtifactBuilder(name)

	artifact = artifact.WithStep(step)
	artifact = artifact.WithSystem(artifactApi.ArtifactSystem_AARCH64_DARWIN)
	artifact = artifact.WithSystem(artifactApi.ArtifactSystem_AARCH64_LINUX)
	artifact = artifact.WithSystem(artifactApi.ArtifactSystem_X8664_DARWIN)
	artifact = artifact.WithSystem(artifactApi.ArtifactSystem_X8664_LINUX)

	return artifact.Build(context)
}
