package artifact

import (
	"errors"
	"fmt"
	"slices"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type ArtifactArgumentBuilder struct {
	Name    string
	Require bool
}

type ArtifactProcessBuilder struct {
	Arguments  []string
	Artifacts  []*string
	Entrypoint string
	Name       string
	Secrets    []*api.ArtifactStepSecret
	Systems    []api.ArtifactSystem
}

type ArtifactSourceBuilder struct {
	Digest   *string
	Excludes []string
	Includes []string
	Name     string
	Path     string
}

type ArtifactStepBuilder struct {
	Arguments    map[api.ArtifactSystem][]string
	Artifacts    map[api.ArtifactSystem][]*string
	Entrypoint   map[api.ArtifactSystem]string
	Environments map[api.ArtifactSystem][]string
	Secrets      map[api.ArtifactSystem][]*api.ArtifactStepSecret
	Script       map[api.ArtifactSystem]string
}

type ArtifactTaskBuilder struct {
	Artifacts []*string
	Name      string
	Script    string
	Secrets   []*api.ArtifactStepSecret
	Systems   []api.ArtifactSystem
}

type ArtifactBuilder struct {
	Aliases []string
	Name    string
	Sources []*api.ArtifactSource
	Steps   []*api.ArtifactStep
	Systems []api.ArtifactSystem
}

type DevenvBuilder struct {
	Artifacts    []*string
	Environments []string
	Name         string
	Secrets      []*api.ArtifactStepSecret
	Systems      []api.ArtifactSystem
}

func NewDevenvBuilder(name string, systems []api.ArtifactSystem) *DevenvBuilder {
	return &DevenvBuilder{
		Artifacts:    []*string{},
		Environments: []string{},
		Name:         name,
		Secrets:      []*api.ArtifactStepSecret{},
		Systems:      systems,
	}
}

func (b *DevenvBuilder) WithArtifacts(artifacts []*string) *DevenvBuilder {
	b.Artifacts = artifacts
	return b
}

func (b *DevenvBuilder) WithEnvironments(envs []string) *DevenvBuilder {
	b.Environments = envs
	return b
}

func (b *DevenvBuilder) WithSecrets(secrets map[string]string) *DevenvBuilder {
	for name, value := range secrets {
		secret := &api.ArtifactStepSecret{Name: name, Value: value}
		if !slices.ContainsFunc(b.Secrets, func(s *api.ArtifactStepSecret) bool { return s.Name == name }) {
			b.Secrets = append(b.Secrets, secret)
		}
	}
	return b
}

func (b *DevenvBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	return ScriptDevenv(ctx, b.Artifacts, b.Environments, b.Name, b.Secrets, b.Systems)
}

// UserenvBuilder builds a user environment activation artifact.
type UserenvBuilder struct {
	Artifacts    []*string
	Environments []string
	Name         string
	Symlinks     map[string]string
	Systems      []api.ArtifactSystem
}

func NewUserenvBuilder(name string, systems []api.ArtifactSystem) *UserenvBuilder {
	return &UserenvBuilder{
		Artifacts:    []*string{},
		Environments: []string{},
		Name:         name,
		Symlinks:     map[string]string{},
		Systems:      systems,
	}
}

func (b *UserenvBuilder) WithArtifacts(artifacts []*string) *UserenvBuilder {
	b.Artifacts = artifacts
	return b
}

func (b *UserenvBuilder) WithEnvironments(envs []string) *UserenvBuilder {
	b.Environments = envs
	return b
}

func (b *UserenvBuilder) WithSymlinks(links map[string]string) *UserenvBuilder {
	if b.Symlinks == nil {
		b.Symlinks = map[string]string{}
	}

	for k, v := range links {
		b.Symlinks[k] = v
	}

	return b
}

func (b *UserenvBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	return ScriptUserenv(ctx, b.Artifacts, b.Environments, b.Name, b.Symlinks, b.Systems)
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

func NewArtifactArgumentBuilder(name string) *ArtifactArgumentBuilder {
	return &ArtifactArgumentBuilder{
		Name:    name,
		Require: false,
	}
}

func (v *ArtifactArgumentBuilder) WithRequire() *ArtifactArgumentBuilder {
	v.Require = true
	return v
}

func (v *ArtifactArgumentBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	variable := ctx.GetVariable(v.Name)

	if v.Require && variable == nil {
		return nil, fmt.Errorf("variable '%s' is required", v.Name)
	}

	return variable, nil
}

func NewArtifactProcessBuilder(name string, entrypoint string, systems []api.ArtifactSystem) *ArtifactProcessBuilder {
	return &ArtifactProcessBuilder{
		Arguments:  []string{},
		Artifacts:  []*string{},
		Entrypoint: entrypoint,
		Name:       name,
		Secrets:    []*api.ArtifactStepSecret{},
		Systems:    systems,
	}
}

func (a *ArtifactProcessBuilder) WithArguments(arguments []string) *ArtifactProcessBuilder {
	a.Arguments = arguments
	return a
}

func (a *ArtifactProcessBuilder) WithArtifacts(artifacts []*string) *ArtifactProcessBuilder {
	for _, artifact := range artifacts {
		if artifact != nil && !slices.Contains(a.Artifacts, artifact) {
			a.Artifacts = append(a.Artifacts, artifact)
		}
	}
	return a
}

func (a *ArtifactProcessBuilder) WithSecrets(secrets []*api.ArtifactStepSecret) *ArtifactProcessBuilder {
	for _, secret := range secrets {
		if !slices.Contains(a.Secrets, secret) {
			a.Secrets = append(a.Secrets, secret)
		}
	}
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

	step, err := Shell(ctx, a.Artifacts, []string{}, scriptBuffer.String(), a.Secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return NewArtifactBuilder(a.Name, steps, a.Systems).
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

func (a *ArtifactSourceBuilder) Build() api.ArtifactSource {
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

func NewArtifactStepBuilder() *ArtifactStepBuilder {
	return &ArtifactStepBuilder{
		Arguments:    make(map[api.ArtifactSystem][]string),
		Artifacts:    make(map[api.ArtifactSystem][]*string),
		Entrypoint:   make(map[api.ArtifactSystem]string),
		Environments: make(map[api.ArtifactSystem][]string),
		Secrets:      make(map[api.ArtifactSystem][]*api.ArtifactStepSecret),
		Script:       make(map[api.ArtifactSystem]string),
	}
}

func (a *ArtifactStepBuilder) WithArguments(arguments []string, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Arguments[system] = arguments
	}
	return a
}

func (a *ArtifactStepBuilder) WithArtifacts(artifacts []*string, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Artifacts[system] = artifacts
	}
	return a
}

func (a *ArtifactStepBuilder) WithEntrypoint(entrypoint string, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Entrypoint[system] = entrypoint
	}
	return a
}

func (a *ArtifactStepBuilder) WithEnvironments(environments []string, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Environments[system] = environments
	}
	return a
}

func (a *ArtifactStepBuilder) WithScript(script string, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		a.Script[system] = script
	}
	return a
}

func (a *ArtifactStepBuilder) WithSecrets(secrets []*api.ArtifactStepSecret, systems []api.ArtifactSystem) *ArtifactStepBuilder {
	for _, system := range systems {
		if _, ok := a.Secrets[system]; !ok {
			a.Secrets[system] = []*api.ArtifactStepSecret{}
		}
		a.Secrets[system] = append(a.Secrets[system], secrets...)
	}
	return a
}

func (a *ArtifactStepBuilder) Build(ctx *config.ConfigContext) (*api.ArtifactStep, error) {
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

	var stepSecrets []*api.ArtifactStepSecret
	if secrets, ok := a.Secrets[stepTarget]; ok {
		stepSecrets = secrets
	}

	var stepScript *string
	if scr, ok := a.Script[stepTarget]; ok {
		stepScript = &scr
	}

	return &api.ArtifactStep{
		Arguments:    stepArguments,
		Artifacts:    stepArtifacts,
		Entrypoint:   stepEntrypoint,
		Environments: stepEnvironments,
		Secrets:      stepSecrets,
		Script:       stepScript,
	}, nil
}

func NewArtifactTaskBuilder(name string, script string, systems []api.ArtifactSystem) *ArtifactTaskBuilder {
	return &ArtifactTaskBuilder{
		Artifacts: []*string{},
		Name:      name,
		Secrets:   []*api.ArtifactStepSecret{},
		Script:    script,
		Systems:   systems,
	}
}

func (a *ArtifactTaskBuilder) WithArtifacts(artifacts []*string) *ArtifactTaskBuilder {
	a.Artifacts = artifacts
	return a
}

func (a *ArtifactTaskBuilder) WithSecrets(secrets []*api.ArtifactStepSecret) *ArtifactTaskBuilder {
	for _, secret := range secrets {
		if !slices.Contains(a.Secrets, secret) {
			a.Secrets = append(a.Secrets, secret)
		}
	}
	return a
}

func (a *ArtifactTaskBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	step, err := Shell(ctx, a.Artifacts, []string{}, a.Script, a.Secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return NewArtifactBuilder(a.Name, steps, a.Systems).
		Build(ctx)
}

func NewArtifactBuilder(name string, steps []*api.ArtifactStep, systems []api.ArtifactSystem) *ArtifactBuilder {
	return &ArtifactBuilder{
		Aliases: []string{},
		Name:    name,
		Sources: []*api.ArtifactSource{},
		Steps:   steps,
		Systems: systems,
	}
}

func (a *ArtifactBuilder) WithAliases(aliases []string) *ArtifactBuilder {
	for _, alias := range aliases {
		if !slices.Contains(a.Aliases, alias) {
			a.Aliases = append(a.Aliases, alias)
		}
	}

	return a
}

func (a *ArtifactBuilder) WithSources(source []*api.ArtifactSource) *ArtifactBuilder {
	for _, s := range source {
		if s != nil && !slices.Contains(a.Sources, s) {
			a.Sources = append(a.Sources, s)
		}
	}

	return a
}

func (a *ArtifactBuilder) WithStep(step *api.ArtifactStep) *ArtifactBuilder {
	if !slices.Contains(a.Steps, step) {
		a.Steps = append(a.Steps, step)
	}

	return a
}

func (a *ArtifactBuilder) WithSystem(system api.ArtifactSystem) *ArtifactBuilder {
	if !slices.Contains(a.Systems, system) {
		a.Systems = append(a.Systems, system)
	}

	return a
}

func (a *ArtifactBuilder) Build(ctx *config.ConfigContext) (*string, error) {
	artifact := api.Artifact{
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

func GetEnvKey(digest *string) string {
	return fmt.Sprintf("$VORPAL_ARTIFACT_%s", *digest)
}
