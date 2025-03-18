package artifact

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/context"
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

# Set backup variables
{{.Backups}}

# Set new variables
{{.Exports}}

# Restore old variables
exit-shell(){{
# Set restore variables
{{.Restores}}

# Set unset variables
{{.Unsets}}
}}

# Run the command
exec "$@"
EOF

chmod +x $VORPAL_WORKSPACE/bin/activate

mkdir -pv $VORPAL_OUTPUT/bin

cp -prv bin "$VORPAL_OUTPUT"
`

func ShellArtifact(ctx *context.ConfigContext, artifacts []*artifactApi.ArtifactId, environments []string, name string) (*artifactApi.ArtifactId, error) {
	backups := []string{
		"export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"",
		"export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"",
		"export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"",
	}

	exports := []string{
		fmt.Sprintf("export PS1=(\"%s\")", name),
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
		backups = append(backups, fmt.Sprintf("export VORPAL_SHELL_BACKUP_%s=\"$%s\"", key, key))
		exports = append(exports, fmt.Sprintf("export %s", envvar))
		restores = append(restores, fmt.Sprintf("export %s=\"$VORPAL_SHELL_BACKUP_%s\"", key, key))
		unsets = append(unsets, fmt.Sprintf("unset VORPAL_SHELL_BACKUP_%s", key))
	}

	scriptTemplate, err := template.New("script").Parse(ShellArtifactScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	scriptTemplateVars := ShellArtifactTemplate{
		Backups:  strings.Join(backups, "\n"),
		Exports:  strings.Join(exports, "\n"),
		Restores: strings.Join(restores, "\n"),
		Unsets:   strings.Join(unsets, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	return AddArtifact(
		ctx,
		artifacts,
		map[string]string{},
		name,
		scriptBuffer.String(),
		[]*artifactApi.ArtifactSourceId{},
		[]string{
			"aarch64-linux",
			"aarch64-macos",
			"x86_64-linux",
			"x86_64-macos",
		},
	), nil
}
