package artifact

import (
	"errors"
	"fmt"
	"slices"
	"strings"
	"text/template"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type ArtifactProcessBuilder struct {
	Arguments  []string
	Artifacts  []*string
	Entrypoint string
	Name       string
}

type ArtifactSourceBuilder struct {
	Digest   *string
	Excludes []string
	Includes []string
	Name     string
	Path     string
}

type ArtifactStepBuilder struct {
	Arguments    map[artifact.ArtifactSystem][]string
	Artifacts    map[artifact.ArtifactSystem][]*string
	Entrypoint   map[artifact.ArtifactSystem]string
	Environments map[artifact.ArtifactSystem][]string
	Script       map[artifact.ArtifactSystem]string
}

type ArtifactTaskBuilder struct {
	Artifacts []*string
	Name      string
	Script    string
}

type ArtifactVariableBuilder struct {
	Encrypt bool
	Name    string
	Require bool
}

type ArtifactBuilder struct {
	Name    string
	Sources []*artifact.ArtifactSource
	Steps   []*artifact.ArtifactStep
	Systems []artifact.ArtifactSystem
}

type ArtifactProcessScriptTemplateVars struct {
	Arguments  string
	Artifacts  string
	Entrypoint string
	Name       string
}

const ArtifactProcessScriptTemplate = `
mkdir -pv $VORPAL_OUTPUT/bin

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

func NewArtifactProcessBuilder(name string, entrypoint string) *ArtifactProcessBuilder {
	return &ArtifactProcessBuilder{
		Arguments:  []string{},
		Artifacts:  []*string{},
		Entrypoint: entrypoint,
		Name:       name,
	}
}

func (a *ArtifactProcessBuilder) WithArguments(arguments []string) *ArtifactProcessBuilder {
	a.Arguments = arguments
	return a
}

func (a *ArtifactProcessBuilder) WithArtifacts(artifacts []*string) *ArtifactProcessBuilder {
	a.Artifacts = artifacts
	return a
}

func (a *ArtifactProcessBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	arguments := strings.Join(a.Arguments, " ")

	artifacts := []string{}

	for _, artifact := range a.Artifacts {
		if artifact != nil {
			artifacts = append(artifacts, fmt.Sprintf("$VORPAL_ARTIFACT_%s/bin", *artifact))
		}
	}

	script, err := template.New("script").Parse(ArtifactProcessScriptTemplate)
	if err != nil {
		return nil, err
	}

	var scriptBuffer strings.Builder

	scriptTemplateVars := ArtifactProcessScriptTemplateVars{
		Arguments:  arguments,
		Artifacts:  strings.Join(artifacts, ":"),
		Entrypoint: a.Entrypoint,
		Name:       a.Name,
	}

	if err := script.Execute(&scriptBuffer, scriptTemplateVars); err != nil {
		return nil, err
	}

	step, err := Shell(ctx, a.Artifacts, []string{}, scriptBuffer.String())
	if err != nil {
		return nil, err
	}

	return NewArtifactBuilder(a.Name).
		WithStep(step).
		WithSystem(artifact.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(artifact.ArtifactSystem_AARCH64_LINUX).
		WithSystem(artifact.ArtifactSystem_X8664_DARWIN).
		WithSystem(artifact.ArtifactSystem_X8664_LINUX).
		Build(ctx)
}

func NewArtifactSourceBuilder(name, path string) *ArtifactSourceBuilder {
	return &ArtifactSourceBuilder{
		Excludes: []string{},
		Digest:   nil,
		Includes: []string{},
		Name:     name,
		Path:     path,
	}
}

func (a *ArtifactSourceBuilder) WithExcludes(excludes []string) *ArtifactSourceBuilder {
	a.Excludes = excludes
	return a
}

func (a *ArtifactSourceBuilder) WithHash(hash string) *ArtifactSourceBuilder {
	a.Digest = &hash
	return a
}

func (a *ArtifactSourceBuilder) WithIncludes(includes []string) *ArtifactSourceBuilder {
	a.Includes = includes
	return a
}

func (a *ArtifactSourceBuilder) Build() artifact.ArtifactSource {
	var digest *string
	if a.Digest != nil {
		digest = a.Digest
	}

	return artifact.ArtifactSource{
		Digest:   digest,
		Includes: a.Includes,
		Excludes: a.Excludes,
		Name:     a.Name,
		Path:     a.Path,
	}
}

func NewArtifactStepBuilder() *ArtifactStepBuilder {
	return &ArtifactStepBuilder{
		Arguments:    make(map[artifact.ArtifactSystem][]string),
		Artifacts:    make(map[artifact.ArtifactSystem][]*string),
		Entrypoint:   make(map[artifact.ArtifactSystem]string),
		Environments: make(map[artifact.ArtifactSystem][]string),
		Script:       make(map[artifact.ArtifactSystem]string),
	}
}

func (a *ArtifactStepBuilder) WithArguments(arguments []string, systems []artifact.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Arguments[system] = arguments
	}
	return a
}

func (a *ArtifactStepBuilder) WithArtifacts(artifacts []*string, systems []artifact.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Artifacts[system] = artifacts
	}
	return a
}

func (a *ArtifactStepBuilder) WithEntrypoint(entrypoint string, systems []artifact.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Entrypoint[system] = entrypoint
	}
	return a
}

func (a *ArtifactStepBuilder) WithEnvironments(environments []string, systems []artifact.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Environments[system] = environments
	}
	return a
}

func (a *ArtifactStepBuilder) WithScript(script string, systems []artifact.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Script[system] = script
	}
	return a
}

func (a *ArtifactStepBuilder) Build(ctx *config.ConfigContext) *artifact.ArtifactStep {
	stepTarget := ctx.GetTarget()

	stepArguments := []string{}
	if args, ok := a.Arguments[stepTarget]; ok {
		stepArguments = args
	}

	stepArtifacts := []string{}
	if arts, ok := a.Artifacts[stepTarget]; ok {
		artifacts := make([]string, len(arts))

		for i, art := range arts {
			if art != nil {
				artifacts[i] = *art
			}
		}

		stepArtifacts = artifacts
	}

	stepEnvironments := []string{}
	if envs, ok := a.Environments[stepTarget]; ok {
		stepEnvironments = envs
	}

	var stepEntrypoint *string
	if entry, ok := a.Entrypoint[stepTarget]; ok {
		stepEntrypoint = &entry
	}

	var stepScript *string
	if scr, ok := a.Script[stepTarget]; ok {
		stepScript = &scr
	}

	return &artifact.ArtifactStep{
		Arguments:    stepArguments,
		Artifacts:    stepArtifacts,
		Entrypoint:   stepEntrypoint,
		Environments: stepEnvironments,
		Script:       stepScript,
	}
}

func NewArtifactTaskBuilder(name string, script string) *ArtifactTaskBuilder {
	return &ArtifactTaskBuilder{
		Artifacts: []*string{},
		Name:      name,
		Script:    script,
	}
}

func (a *ArtifactTaskBuilder) WithArtifacts(artifacts []*string) *ArtifactTaskBuilder {
	a.Artifacts = artifacts
	return a
}

func (a *ArtifactTaskBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	step, err := Shell(ctx, a.Artifacts, []string{}, a.Script)
	if err != nil {
		return nil, err
	}

	return NewArtifactBuilder(a.Name).
		WithStep(step).
		WithSystem(artifact.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(artifact.ArtifactSystem_AARCH64_LINUX).
		WithSystem(artifact.ArtifactSystem_X8664_DARWIN).
		WithSystem(artifact.ArtifactSystem_X8664_LINUX).
		Build(ctx)
}

func NewArtifactVariableBuilder(name string) *ArtifactVariableBuilder {
	return &ArtifactVariableBuilder{
		Encrypt: false,
		Name:    name,
		Require: false,
	}
}

func (v *ArtifactVariableBuilder) WithEncrypt() *ArtifactVariableBuilder {
	v.Encrypt = true
	return v
}

func (v *ArtifactVariableBuilder) WithRequire() *ArtifactVariableBuilder {
	v.Require = true
	return v
}

func (v *ArtifactVariableBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	variable := ctx.GetVariable(v.Name)

	if v.Require && variable == nil {
		return nil, fmt.Errorf("variable '%s' is required", v.Name)
	}

	return variable, nil
}

func NewArtifactBuilder(name string) *ArtifactBuilder {
	return &ArtifactBuilder{
		Name:    name,
		Sources: []*artifact.ArtifactSource{},
		Steps:   []*artifact.ArtifactStep{},
		Systems: []artifact.ArtifactSystem{},
	}
}

func (a *ArtifactBuilder) WithSource(source *artifact.ArtifactSource) *ArtifactBuilder {
	if !slices.Contains(a.Sources, source) {
		a.Sources = append(a.Sources, source)
	}

	return a
}

func (a *ArtifactBuilder) WithStep(step *artifact.ArtifactStep) *ArtifactBuilder {
	if !slices.Contains(a.Steps, step) {
		a.Steps = append(a.Steps, step)
	}

	return a
}

func (a *ArtifactBuilder) WithSystem(system artifact.ArtifactSystem) *ArtifactBuilder {
	if !slices.Contains(a.Systems, system) {
		a.Systems = append(a.Systems, system)
	}

	return a
}

func (a *ArtifactBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	artifactTarget := ctx.GetTarget()

	artifact := artifact.Artifact{
		Name:    a.Name,
		Sources: a.Sources,
		Steps:   a.Steps,
		Systems: a.Systems,
		Target:  artifactTarget,
	}

	if len(artifact.Steps) == 0 {
		return nil, errors.New("artifact must have at least one step")
	}

	return ctx.AddArtifact(&artifact)
}

func GetEnvKey(digest *string) string {
	return fmt.Sprintf("$VORPAL_ARTIFACT_%s", *digest)
}
