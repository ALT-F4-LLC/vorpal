package language

import (
	"fmt"
	"slices"
	"strings"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type TypeScript struct {
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

func NewTypeScript(name string, systems []api.ArtifactSystem) *TypeScript {
	return &TypeScript{
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

func (b *TypeScript) WithAliases(aliases []string) *TypeScript {
	b.aliases = aliases
	return b
}

func (b *TypeScript) WithArtifacts(artifacts []*string) *TypeScript {
	b.artifacts = artifacts
	return b
}

func (b *TypeScript) WithEntrypoint(entrypoint string) *TypeScript {
	b.entrypoint = &entrypoint
	return b
}

func (b *TypeScript) WithEnvironments(environments []string) *TypeScript {
	b.environments = environments
	return b
}

func (b *TypeScript) WithIncludes(includes []string) *TypeScript {
	b.includes = includes
	return b
}

func (b *TypeScript) WithSecrets(secrets map[string]string) *TypeScript {
	for k, v := range secrets {
		if _, exists := b.secrets[k]; !exists {
			b.secrets[k] = v
		}
	}
	return b
}

func (b *TypeScript) WithSourceScripts(scripts []string) *TypeScript {
	for _, script := range scripts {
		if !slices.Contains(b.sourceScripts, script) {
			b.sourceScripts = append(b.sourceScripts, script)
		}
	}
	return b
}

func (b *TypeScript) WithWorkingDir(dir string) *TypeScript {
	b.workingDir = &dir
	return b
}

func (builder *TypeScript) Build(context *config.ConfigContext) (*string, error) {
	// Resolve Bun artifact
	bunDigest, err := artifact.Bun(context)
	if err != nil {
		return nil, err
	}

	bunBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*bunDigest))

	// Build source
	sourcePath := "."

	sourceBuilder := artifact.NewArtifactSource(builder.name, sourcePath)

	if len(builder.includes) > 0 {
		sourceBuilder = sourceBuilder.WithIncludes(builder.includes)
	}

	src := sourceBuilder.Build()
	source := &src

	// Setup step source directory
	stepSourceDir := fmt.Sprintf("%s/source/%s", sourcePath, source.Name)

	if builder.workingDir != nil {
		stepSourceDir = fmt.Sprintf("%s/%s", stepSourceDir, *builder.workingDir)
	}

	// Setup build command
	var stepBuildCommand string

	if builder.entrypoint != nil {
		stepBuildCommand = fmt.Sprintf("mkdir -p $VORPAL_OUTPUT/bin\n\n%s/bun build --compile %s --outfile %s\n\ncp %s $VORPAL_OUTPUT/bin/%s",
			bunBin, *builder.entrypoint, builder.name, builder.name, builder.name)
	} else {
		stepBuildCommand = fmt.Sprintf("mkdir -p $VORPAL_OUTPUT\n\n%s/bun x tsc --project tsconfig.json --outDir dist\n\ncp package.json $VORPAL_OUTPUT/\ncp -r dist $VORPAL_OUTPUT/\ncp -r node_modules $VORPAL_OUTPUT/",
			bunBin)
	}

	// Build step script
	stepSourceScripts := strings.Join(builder.sourceScripts, "\n")

	stepScript := fmt.Sprintf("pushd %s\n\n%s\n\n%s/bun install --frozen-lockfile\n\n%s",
		stepSourceDir, stepSourceScripts, bunBin, stepBuildCommand)

	// Build artifacts list
	var artifacts []*string
	artifacts = append(artifacts, bunDigest)
	artifacts = append(artifacts, builder.artifacts...)

	// Build environments
	environments := []string{
		fmt.Sprintf("PATH=%s", bunBin),
	}

	for _, env := range builder.environments {
		environments = append(environments, env)
	}

	// Create step and artifact
	sources := []*api.ArtifactSource{source}

	step, err := artifact.Shell(context, artifacts, environments, stepScript, artifact.SecretsToProto(builder.secrets))
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifact(builder.name, steps, builder.systems).
		WithAliases(builder.aliases).
		WithSources(sources).
		Build(context)
}

// ---------------------------------------------------------------------------
// TypeScript Development Environment
// ---------------------------------------------------------------------------

type TypeScriptDevelopmentEnvironment struct {
	artifacts    []*string
	environments []string
	name         string
	secrets      map[string]string
	systems      []api.ArtifactSystem
}

func NewTypeScriptDevelopmentEnvironment(name string, systems []api.ArtifactSystem) *TypeScriptDevelopmentEnvironment {
	return &TypeScriptDevelopmentEnvironment{
		artifacts:    []*string{},
		environments: []string{},
		name:         name,
		secrets:      map[string]string{},
		systems:      systems,
	}
}

func (b *TypeScriptDevelopmentEnvironment) WithArtifacts(artifacts []*string) *TypeScriptDevelopmentEnvironment {
	b.artifacts = append(b.artifacts, artifacts...)
	return b
}

func (b *TypeScriptDevelopmentEnvironment) WithEnvironments(environments []string) *TypeScriptDevelopmentEnvironment {
	b.environments = append(b.environments, environments...)
	return b
}

func (b *TypeScriptDevelopmentEnvironment) WithSecrets(secrets map[string]string) *TypeScriptDevelopmentEnvironment {
	for k, v := range secrets {
		if _, exists := b.secrets[k]; !exists {
			b.secrets[k] = v
		}
	}
	return b
}

func (b *TypeScriptDevelopmentEnvironment) Build(context *config.ConfigContext) (*string, error) {
	bun, err := artifact.Bun(context)
	if err != nil {
		return nil, err
	}

	artifacts := []*string{bun}
	artifacts = append(artifacts, b.artifacts...)

	devenv := artifact.NewDevelopmentEnvironment(b.name, b.systems).
		WithArtifacts(artifacts).
		WithEnvironments(b.environments)

	if len(b.secrets) > 0 {
		devenv = devenv.WithSecrets(b.secrets)
	}

	return devenv.Build(context)
}
