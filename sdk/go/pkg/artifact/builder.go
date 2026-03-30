package artifact

import (
	"bytes"
	"errors"
	"fmt"
	"slices"
	"sort"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

// SortedKeys returns the keys of a map in sorted order for deterministic iteration.
func SortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// SecretsToProto converts a map[string]string of secrets to a sorted slice of proto objects.
func SecretsToProto(secrets map[string]string) []*api.ArtifactStepSecret {
	result := make([]*api.ArtifactStepSecret, 0, len(secrets))
	for _, name := range SortedKeys(secrets) {
		result = append(result, &api.ArtifactStepSecret{Name: name, Value: secrets[name]})
	}
	return result
}

type Argument struct {
	Name    string
	Require bool
}

type ArtifactSource struct {
	Digest   *string
	Excludes []string
	Includes []string
	Name     string
	Path     string
}

type ArtifactStep struct {
	Arguments    []string
	Artifacts    []*string
	Entrypoint   string
	Environments []string
	Secrets      []*api.ArtifactStepSecret
	Script       string
}

type Artifact struct {
	Aliases []string
	Name    string
	Sources []*api.ArtifactSource
	Steps   []*api.ArtifactStep
	Systems []api.ArtifactSystem
}

type Job struct {
	Artifacts []*string
	Name      string
	Script    string
	Secrets   map[string]string
	Systems   []api.ArtifactSystem
}

type Process struct {
	Arguments  []string
	Artifacts  []*string
	Entrypoint string
	Name       string
	Secrets    map[string]string
	Systems    []api.ArtifactSystem
}

type ProcessScriptTemplateVars struct {
	Arguments  string
	Artifacts  string
	Entrypoint string
	Name       string
}

type DevelopmentEnvironment struct {
	Artifacts    []*string
	Environments []string
	Name         string
	Secrets      map[string]string
	Systems      []api.ArtifactSystem
}

type DevelopmentEnvironmentTemplateArgs struct {
	Backups  string
	Exports  string
	Restores string
	Unsets   string
}

type UserEnvironment struct {
	Artifacts    []*string
	Environments []string
	Name         string
	Symlinks     map[string]string
	Systems      []api.ArtifactSystem
}

type UserEnvironmentTemplateArgs struct {
	Environments       string
	Path               string
	SymlinksActivate   string
	SymlinksCheck      string
	SymlinksDeactivate string
}

const ProcessScriptTemplate = `mkdir -p $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/{{.Name}}-logs << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/logs.txt ]; then
    tail -f $VORPAL_OUTPUT/logs.txt
else
    echo "No logs found"
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/{{.Name}}-logs

cat > $VORPAL_OUTPUT/bin/{{.Name}}-stop << "EOF"
#!/bin/bash
set -euo pipefail

if [ -f $VORPAL_OUTPUT/pid ]; then
    kill $(cat $VORPAL_OUTPUT/pid)
    rm -rf $VORPAL_OUTPUT/pid
fi
EOF

chmod +x $VORPAL_OUTPUT/bin/{{.Name}}-stop

cat > $VORPAL_OUTPUT/bin/{{.Name}}-start << "EOF"
#!/bin/bash
set -euo pipefail

export PATH={{.Artifacts}}:$PATH

$VORPAL_OUTPUT/bin/{{.Name}}-stop

echo "Process: {{.Entrypoint}} {{.Arguments}}"

nohup {{.Entrypoint}} {{.Arguments}} > $VORPAL_OUTPUT/logs.txt 2>&1 &

PROCESS_PID=$!

echo "Process ID: $PROCESS_PID"

echo $PROCESS_PID > $VORPAL_OUTPUT/pid

echo "Process commands:"
echo "- {{.Name}}-logs (tail logs)"
echo "- {{.Name}}-stop (stop process)"
echo "- {{.Name}}-start (start process)"
EOF

chmod +x $VORPAL_OUTPUT/bin/{{.Name}}-start`

const ScriptDevelopmentEnvironmentTemplate = `mkdir -p $VORPAL_WORKSPACE/bin

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

mkdir -p $VORPAL_OUTPUT/bin

cp -pr bin "$VORPAL_OUTPUT"`

const ScriptUserEnvironmentTemplate = `mkdir -p $VORPAL_OUTPUT/bin

cat > $VORPAL_OUTPUT/bin/vorpal-activate-shell << "EOF"
{{.Environments}}
export PATH="$VORPAL_OUTPUT/bin:{{.Path}}:$PATH"
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
{{.SymlinksDeactivate}}
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-activate-symlinks << "EOF"
#!/bin/bash
set -euo pipefail
{{.SymlinksCheck}}
{{.SymlinksActivate}}
EOF

cat > $VORPAL_OUTPUT/bin/vorpal-activate << "EOF"
#!/bin/bash
set -euo pipefail

echo "Deactivating previous symlinks..."

if [ -f $HOME/.vorpal/bin/vorpal-deactivate-symlinks ]; then
    $HOME/.vorpal/bin/vorpal-deactivate-symlinks
fi

echo "Activating symlinks..."

$VORPAL_OUTPUT/bin/vorpal-activate-symlinks

echo "Vorpal userenv installed. Run 'source vorpal-activate-shell' to activate."

ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-shell $HOME/.vorpal/bin/vorpal-activate-shell
ln -sf $VORPAL_OUTPUT/bin/vorpal-activate-symlinks $HOME/.vorpal/bin/vorpal-activate-symlinks
ln -sf $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks $HOME/.vorpal/bin/vorpal-deactivate-symlinks
EOF


chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-shell
chmod +x $VORPAL_OUTPUT/bin/vorpal-deactivate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate-symlinks
chmod +x $VORPAL_OUTPUT/bin/vorpal-activate`

func GetEnvKey(digest string) string {
	return fmt.Sprintf("$VORPAL_ARTIFACT_%s", digest)
}

func NewArtifactArgument(name string) *Argument {
	return &Argument{
		Name:    name,
		Require: false,
	}
}

func (v *Argument) WithRequire() *Argument {
	v.Require = true
	return v
}

func (v *Argument) Build(ctx *config.ConfigContext) (*string, error) {
	variable := ctx.GetVariable(v.Name)

	if v.Require && variable == nil {
		return nil, fmt.Errorf("variable '%s' is required", v.Name)
	}

	return variable, nil
}

func NewProcess(name string, entrypoint string, systems []api.ArtifactSystem) *Process {
	return &Process{
		Arguments:  []string{},
		Artifacts:  []*string{},
		Entrypoint: entrypoint,
		Name:       name,
		Secrets:    map[string]string{},
		Systems:    systems,
	}
}

func (a *Process) WithArguments(arguments []string) *Process {
	a.Arguments = arguments
	return a
}

func (a *Process) WithArtifacts(artifacts []*string) *Process {
	for _, artifact := range artifacts {
		if artifact != nil && !slices.Contains(a.Artifacts, artifact) {
			a.Artifacts = append(a.Artifacts, artifact)
		}
	}
	return a
}

func (a *Process) WithSecrets(secrets map[string]string) *Process {
	for k, v := range secrets {
		if _, exists := a.Secrets[k]; !exists {
			a.Secrets[k] = v
		}
	}
	return a
}

func (a *Process) Build(ctx *config.ConfigContext) (*string, error) {
	secrets := SecretsToProto(a.Secrets)

	arguments := strings.Join(a.Arguments, " ")

	artifacts := []string{}

	for _, artifact := range a.Artifacts {
		if artifact != nil {
			artifacts = append(artifacts, fmt.Sprintf("$VORPAL_ARTIFACT_%s/bin", *artifact))
		}
	}

	script, err := template.New("script").Parse(ProcessScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer strings.Builder

	scriptTemplateVars := ProcessScriptTemplateVars{
		Arguments:  arguments,
		Artifacts:  strings.Join(artifacts, ":"),
		Entrypoint: a.Entrypoint,
		Name:       a.Name,
	}

	if err := script.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	step, err := Shell(ctx, a.Artifacts, []string{}, scriptBuffer.String(), secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return NewArtifact(a.Name, steps, a.Systems).
		Build(ctx)
}

func NewArtifactSource(name, path string) *ArtifactSource {
	return &ArtifactSource{
		Excludes: []string{},
		Digest:   nil,
		Includes: []string{},
		Name:     name,
		Path:     path,
	}
}

func (a *ArtifactSource) WithExcludes(excludes []string) *ArtifactSource {
	a.Excludes = excludes
	return a
}

func (a *ArtifactSource) WithDigest(digest string) *ArtifactSource {
	a.Digest = &digest
	return a
}

func (a *ArtifactSource) WithIncludes(includes []string) *ArtifactSource {
	a.Includes = includes
	return a
}

func (a *ArtifactSource) Build() api.ArtifactSource {
	var digest *string
	if a.Digest != nil {
		digest = a.Digest
	}

	return api.ArtifactSource{
		Digest:   digest,
		Includes: a.Includes,
		Excludes: a.Excludes,
		Name:     a.Name,
		Path:     a.Path,
	}
}

func NewArtifactStep(entrypoint string) *ArtifactStep {
	return &ArtifactStep{
		Arguments:    []string{},
		Artifacts:    []*string{},
		Entrypoint:   entrypoint,
		Environments: []string{},
		Secrets:      []*api.ArtifactStepSecret{},
	}
}

func (a *ArtifactStep) WithArguments(arguments []string) *ArtifactStep {
	a.Arguments = arguments
	return a
}

func (a *ArtifactStep) WithArtifacts(artifacts []*string) *ArtifactStep {
	a.Artifacts = artifacts
	return a
}

func (a *ArtifactStep) WithEnvironments(environments []string) *ArtifactStep {
	a.Environments = environments
	return a
}

func (a *ArtifactStep) WithScript(script string) *ArtifactStep {
	a.Script = script
	return a
}

func (a *ArtifactStep) WithSecrets(secrets []*api.ArtifactStepSecret) *ArtifactStep {
	a.Secrets = append(a.Secrets, secrets...)
	return a
}

func (a *ArtifactStep) Build() *api.ArtifactStep {
	stepArtifacts := make([]string, 0, len(a.Artifacts))
	for _, art := range a.Artifacts {
		if art != nil {
			stepArtifacts = append(stepArtifacts, *art)
		}
	}

	step := &api.ArtifactStep{
		Arguments:    a.Arguments,
		Artifacts:    stepArtifacts,
		Environments: a.Environments,
		Secrets:      a.Secrets,
	}

	if a.Entrypoint != "" {
		entrypoint := a.Entrypoint
		step.Entrypoint = &entrypoint
	}

	if a.Script != "" {
		script := a.Script
		step.Script = &script
	}

	return step
}

func NewJob(name string, script string, systems []api.ArtifactSystem) *Job {
	return &Job{
		Artifacts: []*string{},
		Name:      name,
		Secrets:   map[string]string{},
		Script:    script,
		Systems:   systems,
	}
}

func (a *Job) WithArtifacts(artifacts []*string) *Job {
	a.Artifacts = artifacts
	return a
}

func (a *Job) WithSecrets(secrets map[string]string) *Job {
	for k, v := range secrets {
		if _, exists := a.Secrets[k]; !exists {
			a.Secrets[k] = v
		}
	}
	return a
}

func (a *Job) Build(ctx *config.ConfigContext) (*string, error) {
	secrets := SecretsToProto(a.Secrets)

	step, err := Shell(ctx, a.Artifacts, []string{}, a.Script, secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return NewArtifact(a.Name, steps, a.Systems).
		Build(ctx)
}

func NewArtifact(name string, steps []*api.ArtifactStep, systems []api.ArtifactSystem) *Artifact {
	return &Artifact{
		Aliases: []string{},
		Name:    name,
		Sources: []*api.ArtifactSource{},
		Steps:   steps,
		Systems: systems,
	}
}

func (a *Artifact) WithAliases(aliases []string) *Artifact {
	for _, alias := range aliases {
		if !slices.Contains(a.Aliases, alias) {
			a.Aliases = append(a.Aliases, alias)
		}
	}

	return a
}

func (a *Artifact) WithSources(source []*api.ArtifactSource) *Artifact {
	for _, s := range source {
		if s != nil && !slices.Contains(a.Sources, s) {
			a.Sources = append(a.Sources, s)
		}
	}

	return a
}

func (a *Artifact) Build(ctx *config.ConfigContext) (*string, error) {
	artifact := api.Artifact{
		Aliases: a.Aliases,
		Name:    a.Name,
		Sources: a.Sources,
		Steps:   a.Steps,
		Systems: a.Systems,
		Target:  ctx.GetTarget(),
	}

	if len(artifact.Steps) == 0 {
		return nil, errors.New("artifact must have at least one step")
	}

	return ctx.AddArtifact(&artifact)
}

func NewDevelopmentEnvironment(name string, systems []api.ArtifactSystem) *DevelopmentEnvironment {
	return &DevelopmentEnvironment{
		Artifacts:    []*string{},
		Environments: []string{},
		Name:         name,
		Secrets:      map[string]string{},
		Systems:      systems,
	}
}

func (b *DevelopmentEnvironment) WithArtifacts(artifacts []*string) *DevelopmentEnvironment {
	b.Artifacts = artifacts
	return b
}

func (b *DevelopmentEnvironment) WithEnvironments(envs []string) *DevelopmentEnvironment {
	b.Environments = envs
	return b
}

func (b *DevelopmentEnvironment) WithSecrets(secrets map[string]string) *DevelopmentEnvironment {
	for k, v := range secrets {
		if _, exists := b.Secrets[k]; !exists {
			b.Secrets[k] = v
		}
	}
	return b
}

func (b *DevelopmentEnvironment) Build(ctx *config.ConfigContext) (*string, error) {
	backups := []string{
		"export VORPAL_SHELL_BACKUP_PATH=\"$PATH\"",
		"export VORPAL_SHELL_BACKUP_PS1=\"$PS1\"",
		"export VORPAL_SHELL_BACKUP_VORPAL_SHELL=\"$VORPAL_SHELL\"",
	}

	exports := []string{
		fmt.Sprintf("export PS1=\"(%s) $PS1\"", b.Name),
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

	for _, envvar := range b.Environments {
		parts := strings.SplitN(envvar, "=", 2)
		if len(parts) != 2 {
			continue
		}

		key := parts[0]

		if key == "PATH" {
			continue
		}

		backups = append(backups, fmt.Sprintf("export VORPAL_SHELL_BACKUP_%s=\"$%s\"", key, key))
		exports = append(exports, fmt.Sprintf("export %s", envvar))
		restores = append(restores, fmt.Sprintf("export %s=\"$VORPAL_SHELL_BACKUP_%s\"", key, key))
		unsets = append(unsets, fmt.Sprintf("unset VORPAL_SHELL_BACKUP_%s", key))
	}

	// Setup path

	stepPathArtifacts := make([]string, 0)

	for _, artifact := range b.Artifacts {
		if artifact != nil {
			stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", GetEnvKey(*artifact)))
		}
	}

	stepPath := strings.Join(stepPathArtifacts, ":")

	for _, envvar := range b.Environments {
		if pathValue, ok := strings.CutPrefix(envvar, "PATH="); ok {
			stepPath = fmt.Sprintf("%s:%s", pathValue, stepPath)
		}
	}

	exports = append(exports, fmt.Sprintf("export PATH=%s:$PATH", stepPath))

	// Setup script

	scriptTemplate, err := template.New("script").Parse(ScriptDevelopmentEnvironmentTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	stepScriptVars := DevelopmentEnvironmentTemplateArgs{
		Backups:  strings.Join(backups, "\n"),
		Exports:  strings.Join(exports, "\n"),
		Restores: strings.Join(restores, "\n"),
		Unsets:   strings.Join(unsets, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, stepScriptVars); err != nil {
		return nil, err
	}

	stepScript := scriptBuffer.String()

	secrets := SecretsToProto(b.Secrets)

	step, err := Shell(ctx, b.Artifacts, []string{}, stepScript, secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	artifact := NewArtifact(b.Name, steps, b.Systems)

	return artifact.Build(ctx)
}

func NewUserEnvironment(name string, systems []api.ArtifactSystem) *UserEnvironment {
	return &UserEnvironment{
		Artifacts:    []*string{},
		Environments: []string{},
		Name:         name,
		Symlinks:     map[string]string{},
		Systems:      systems,
	}
}

func (b *UserEnvironment) WithArtifacts(artifacts []*string) *UserEnvironment {
	b.Artifacts = artifacts
	return b
}

func (b *UserEnvironment) WithEnvironments(envs []string) *UserEnvironment {
	b.Environments = envs
	return b
}

func (b *UserEnvironment) WithSymlinks(links map[string]string) *UserEnvironment {
	if b.Symlinks == nil {
		b.Symlinks = map[string]string{}
	}

	for k, v := range links {
		b.Symlinks[k] = v
	}

	return b
}

func (b *UserEnvironment) Build(ctx *config.ConfigContext) (*string, error) {
	// Setup path

	stepPathArtifacts := make([]string, 0)

	for _, artifact := range b.Artifacts {
		if artifact != nil {
			stepPathArtifacts = append(stepPathArtifacts, fmt.Sprintf("%s/bin", GetEnvKey(*artifact)))
		}
	}

	stepEnvironments := make([]string, 0)
	stepPath := strings.Join(stepPathArtifacts, ":")

	for _, envvar := range b.Environments {
		if pathValue, ok := strings.CutPrefix(envvar, "PATH="); ok {
			stepPath = fmt.Sprintf("%s:%s", pathValue, stepPath)
			continue
		}

		stepEnvironments = append(stepEnvironments, envvar)
	}

	// Setup script

	scriptTemplate, err := template.New("script").Parse(ScriptUserEnvironmentTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer bytes.Buffer

	symlinksActivate := make([]string, 0)
	symlinksCheck := make([]string, 0)
	symlinksDeactivate := make([]string, 0)

	for _, source := range SortedKeys(b.Symlinks) {
		target := b.Symlinks[source]
		symlinksActivate = append(symlinksActivate, fmt.Sprintf("ln -s %s %s", source, target))
		symlinksCheck = append(symlinksCheck, fmt.Sprintf("if [ -f %s ]; then echo \"ERROR: Symlink target exists -> %s\" && exit 1; fi", target, target))
		symlinksDeactivate = append(symlinksDeactivate, fmt.Sprintf("rm -f %s", target))
	}

	environmentsExport := make([]string, 0)

	for _, envvar := range b.Environments {
		environmentsExport = append(environmentsExport, fmt.Sprintf("export %s", envvar))
	}

	stepScriptVars := UserEnvironmentTemplateArgs{
		Environments:       strings.Join(stepEnvironments, "\n"),
		Path:               stepPath,
		SymlinksActivate:   strings.Join(symlinksActivate, "\n"),
		SymlinksCheck:      strings.Join(symlinksCheck, "\n"),
		SymlinksDeactivate: strings.Join(symlinksDeactivate, "\n"),
	}

	if err := scriptTemplate.Execute(&scriptBuffer, stepScriptVars); err != nil {
		return nil, err
	}

	stepScript := scriptBuffer.String()

	step, err := Shell(ctx, b.Artifacts, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	artifact := NewArtifact(b.Name, steps, b.Systems)

	return artifact.Build(ctx)
}
