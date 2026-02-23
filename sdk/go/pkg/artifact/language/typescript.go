package language

import (
	"bytes"
	"fmt"
	"slices"
	"strings"
	"text/template"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

type TypeScriptScriptTemplateArgs struct {
	BunBin            string
	Entrypoint        string
	Name              string
	NodeModulesScript string
	SourceDir         string
	SourceScripts     string
}

type TypeScript struct {
	artifacts     []*string
	bun           *string
	entrypoint    *string
	environments  []string
	excludes      []string
	includes      []string
	name          string
	nodeModules   map[string]*string
	secrets       []*api.ArtifactStepSecret
	source        *api.ArtifactSource
	sourceScripts []string
	systems       []api.ArtifactSystem
}

const TypeScriptScriptTemplate = `
pushd "{{.SourceDir}}"

mkdir -p "$VORPAL_OUTPUT/bin"
{{- if .SourceScripts}}
{{.SourceScripts}}
{{- end}}
{{- if .NodeModulesScript}}
{{.NodeModulesScript}}
{{- end}}

{{.BunBin}}/bun install --frozen-lockfile
{{.BunBin}}/bun build --compile "{{.Entrypoint}}" --outfile "$VORPAL_OUTPUT/bin/{{.Name}}"`

func NewTypeScript(name string, systems []api.ArtifactSystem) *TypeScript {
	return &TypeScript{
		artifacts:     []*string{},
		bun:           nil,
		entrypoint:    nil,
		environments:  []string{},
		excludes:      []string{},
		includes:      []string{},
		name:          name,
		nodeModules:   map[string]*string{},
		secrets:       []*api.ArtifactStepSecret{},
		source:        nil,
		sourceScripts: []string{},
		systems:       systems,
	}
}

func (b *TypeScript) WithArtifacts(artifacts []*string) *TypeScript {
	b.artifacts = artifacts
	return b
}

func (b *TypeScript) WithBun(bun *string) *TypeScript {
	b.bun = bun
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

func (b *TypeScript) WithExcludes(excludes []string) *TypeScript {
	b.excludes = excludes
	return b
}

func (b *TypeScript) WithIncludes(includes []string) *TypeScript {
	b.includes = includes
	return b
}

func (b *TypeScript) WithSecrets(secrets map[string]string) *TypeScript {
	for _, name := range artifact.SortedKeys(secrets) {
		value := secrets[name]
		secret := &api.ArtifactStepSecret{
			Name:  name,
			Value: value,
		}

		if slices.ContainsFunc(b.secrets, func(s *api.ArtifactStepSecret) bool { return s.Name == name }) {
			continue
		}

		b.secrets = append(b.secrets, secret)
	}

	return b
}

func (b *TypeScript) WithSource(source *api.ArtifactSource) *TypeScript {
	b.source = source
	return b
}

func (b *TypeScript) WithSourceScript(script string) *TypeScript {
	if !slices.Contains(b.sourceScripts, script) {
		b.sourceScripts = append(b.sourceScripts, script)
	}
	return b
}

func (b *TypeScript) WithNodeModule(packageName string, digest *string) *TypeScript {
	b.nodeModules[packageName] = digest
	return b
}

func (b *TypeScript) WithNodeModules(modules map[string]*string) *TypeScript {
	for name, digest := range modules {
		b.nodeModules[name] = digest
	}
	return b
}

func (builder *TypeScript) Build(context *config.ConfigContext) (*string, error) {
	// Sort secrets for deterministic output
	slices.SortFunc(builder.secrets, func(a, b *api.ArtifactStepSecret) int {
		return strings.Compare(a.Name, b.Name)
	})

	// Resolve Bun artifact
	var bunDigest *string

	if builder.bun != nil {
		bunDigest = builder.bun
	} else {
		bun, err := artifact.Bun(context)
		if err != nil {
			return nil, err
		}
		bunDigest = bun
	}

	bunBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*bunDigest))

	// Build source
	sourcePath := "."

	var source *api.ArtifactSource

	if builder.source != nil {
		source = builder.source
	} else {
		sourceBuilder := artifact.NewArtifactSource(builder.name, sourcePath)

		if len(builder.includes) > 0 {
			sourceBuilder = sourceBuilder.WithIncludes(builder.includes)
		}

		if len(builder.excludes) > 0 {
			sourceBuilder = sourceBuilder.WithExcludes(builder.excludes)
		}

		src := sourceBuilder.Build()

		source = &src
	}

	// Resolve entrypoint
	entrypoint := fmt.Sprintf("src/%s.ts", builder.name)
	if builder.entrypoint != nil {
		entrypoint = *builder.entrypoint
	}

	// Build source scripts
	sourceScripts := ""
	if len(builder.sourceScripts) > 0 {
		sourceScripts = strings.Join(builder.sourceScripts, "\n")
	}

	// Generate node modules symlink script
	nodeModulesScript := ""
	if len(builder.nodeModules) > 0 {
		nodeModuleLines := []string{"mkdir -p node_modules"}
		for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
			digest := builder.nodeModules[packageName]
			if digest == nil {
				return nil, fmt.Errorf("node module %q has nil digest", packageName)
			}
			envKey := artifact.GetEnvKey(*digest)
			if strings.Contains(packageName, "/") {
				// Scoped package like @vorpal/sdk
				scope := strings.Split(packageName, "/")[0]
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("mkdir -p node_modules/%s", scope))
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("ln -sf %s node_modules/%s", envKey, packageName))
			} else {
				// Unscoped package like lodash
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("ln -sf %s node_modules/%s", envKey, packageName))
			}
		}
		nodeModulesScript = strings.Join(nodeModuleLines, "\n")
	}

	// Generate build script
	stepScriptData := TypeScriptScriptTemplateArgs{
		BunBin:            bunBin,
		Entrypoint:        entrypoint,
		Name:              builder.name,
		NodeModulesScript: nodeModulesScript,
		SourceDir:         fmt.Sprintf("./source/%s", source.Name),
		SourceScripts:     sourceScripts,
	}

	tmpl, err := template.New("script").Parse(TypeScriptScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	if err := tmpl.Execute(&stepScriptBuffer, stepScriptData); err != nil {
		return nil, err
	}

	stepScript := stepScriptBuffer.String()

	// Build artifacts list
	var artifacts []*string
	artifacts = append(artifacts, bunDigest)
	artifacts = append(artifacts, builder.artifacts...)

	// Add node module artifact digests
	for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
		digest := builder.nodeModules[packageName]
		if digest == nil {
			return nil, fmt.Errorf("node module %q has nil digest", packageName)
		}
		artifacts = append(artifacts, digest)
	}

	// Build environments
	environments := []string{
		fmt.Sprintf("PATH=%s", bunBin),
	}

	for _, env := range builder.environments {
		environments = append(environments, env)
	}

	// Create step and artifact
	sources := []*api.ArtifactSource{source}

	step, err := artifact.Shell(context, artifacts, environments, stepScript, builder.secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifact(builder.name, steps, builder.systems).
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

// ---------------------------------------------------------------------------
// TypeScript Library
// ---------------------------------------------------------------------------

type TypeScriptLibraryScriptTemplateArgs struct {
	BuildCommand      string
	BunBin            string
	NodeModulesScript string
	SourceDir         string
	SourceScripts     string
}

const TypeScriptLibraryScriptTemplate = `pushd "{{.SourceDir}}"
{{- if .SourceScripts}}
{{.SourceScripts}}
{{- end}}
{{- if .NodeModulesScript}}
{{.NodeModulesScript}}
{{- end}}

{{.BunBin}}/bun install --frozen-lockfile
{{.BunBin}}/{{.BuildCommand}}

mkdir -p "$VORPAL_OUTPUT"
cp package.json "$VORPAL_OUTPUT/"
cp -r dist "$VORPAL_OUTPUT/"
cp -r node_modules "$VORPAL_OUTPUT/"`

type TypeScriptLibrary struct {
	artifacts     []*string
	buildCommand  *string
	bun           *string
	environments  []string
	excludes      []string
	includes      []string
	name          string
	nodeModules   map[string]*string
	secrets       []*api.ArtifactStepSecret
	source        *api.ArtifactSource
	sourceScripts []string
	systems       []api.ArtifactSystem
}

func NewTypeScriptLibrary(name string, systems []api.ArtifactSystem) *TypeScriptLibrary {
	return &TypeScriptLibrary{
		artifacts:     []*string{},
		buildCommand:  nil,
		bun:           nil,
		environments:  []string{},
		excludes:      []string{},
		includes:      []string{},
		name:          name,
		nodeModules:   map[string]*string{},
		secrets:       []*api.ArtifactStepSecret{},
		source:        nil,
		sourceScripts: []string{},
		systems:       systems,
	}
}

func (b *TypeScriptLibrary) WithArtifacts(artifacts []*string) *TypeScriptLibrary {
	b.artifacts = artifacts
	return b
}

func (b *TypeScriptLibrary) WithBuildCommand(cmd string) *TypeScriptLibrary {
	b.buildCommand = &cmd
	return b
}

func (b *TypeScriptLibrary) WithBun(bun *string) *TypeScriptLibrary {
	b.bun = bun
	return b
}

func (b *TypeScriptLibrary) WithEnvironments(environments []string) *TypeScriptLibrary {
	b.environments = environments
	return b
}

func (b *TypeScriptLibrary) WithExcludes(excludes []string) *TypeScriptLibrary {
	b.excludes = excludes
	return b
}

func (b *TypeScriptLibrary) WithIncludes(includes []string) *TypeScriptLibrary {
	b.includes = includes
	return b
}

func (b *TypeScriptLibrary) WithNodeModule(packageName string, digest *string) *TypeScriptLibrary {
	b.nodeModules[packageName] = digest
	return b
}

func (b *TypeScriptLibrary) WithNodeModules(modules map[string]*string) *TypeScriptLibrary {
	for name, digest := range modules {
		b.nodeModules[name] = digest
	}
	return b
}

func (b *TypeScriptLibrary) WithSecrets(secrets map[string]string) *TypeScriptLibrary {
	for _, name := range artifact.SortedKeys(secrets) {
		value := secrets[name]
		secret := &api.ArtifactStepSecret{
			Name:  name,
			Value: value,
		}

		if slices.ContainsFunc(b.secrets, func(s *api.ArtifactStepSecret) bool { return s.Name == name }) {
			continue
		}

		b.secrets = append(b.secrets, secret)
	}

	return b
}

func (b *TypeScriptLibrary) WithSource(source *api.ArtifactSource) *TypeScriptLibrary {
	b.source = source
	return b
}

func (b *TypeScriptLibrary) WithSourceScript(script string) *TypeScriptLibrary {
	if !slices.Contains(b.sourceScripts, script) {
		b.sourceScripts = append(b.sourceScripts, script)
	}
	return b
}

func (builder *TypeScriptLibrary) Build(context *config.ConfigContext) (*string, error) {
	// Sort secrets for deterministic output
	slices.SortFunc(builder.secrets, func(a, b *api.ArtifactStepSecret) int {
		return strings.Compare(a.Name, b.Name)
	})

	// Resolve Bun artifact
	var bunDigest *string

	if builder.bun != nil {
		bunDigest = builder.bun
	} else {
		bun, err := artifact.Bun(context)
		if err != nil {
			return nil, err
		}
		bunDigest = bun
	}

	bunBin := fmt.Sprintf("%s/bin", artifact.GetEnvKey(*bunDigest))

	// Build source
	sourcePath := "."

	var source *api.ArtifactSource

	if builder.source != nil {
		source = builder.source
	} else {
		sourceBuilder := artifact.NewArtifactSource(builder.name, sourcePath)

		if len(builder.includes) > 0 {
			sourceBuilder = sourceBuilder.WithIncludes(builder.includes)
		}

		if len(builder.excludes) > 0 {
			sourceBuilder = sourceBuilder.WithExcludes(builder.excludes)
		}

		src := sourceBuilder.Build()

		source = &src
	}

	// Resolve build command
	buildCommand := "bun run build"
	if builder.buildCommand != nil {
		buildCommand = *builder.buildCommand
	}

	// Build source scripts
	sourceScripts := ""
	if len(builder.sourceScripts) > 0 {
		sourceScripts = strings.Join(builder.sourceScripts, "\n")
	}

	// Generate node modules symlink script
	nodeModulesScript := ""
	if len(builder.nodeModules) > 0 {
		nodeModuleLines := []string{"mkdir -p node_modules"}
		for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
			digest := builder.nodeModules[packageName]
			if digest == nil {
				return nil, fmt.Errorf("node module %q has nil digest", packageName)
			}
			envKey := artifact.GetEnvKey(*digest)
			if strings.Contains(packageName, "/") {
				// Scoped package like @vorpal/sdk
				scope := strings.Split(packageName, "/")[0]
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("mkdir -p node_modules/%s", scope))
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("ln -sf %s node_modules/%s", envKey, packageName))
			} else {
				// Unscoped package like lodash
				nodeModuleLines = append(nodeModuleLines, fmt.Sprintf("ln -sf %s node_modules/%s", envKey, packageName))
			}
		}
		nodeModulesScript = strings.Join(nodeModuleLines, "\n")
	}

	// Generate build script
	stepScriptData := TypeScriptLibraryScriptTemplateArgs{
		BuildCommand:      buildCommand,
		BunBin:            bunBin,
		NodeModulesScript: nodeModulesScript,
		SourceDir:         fmt.Sprintf("./source/%s", source.Name),
		SourceScripts:     sourceScripts,
	}

	tmpl, err := template.New("script").Parse(TypeScriptLibraryScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	if err := tmpl.Execute(&stepScriptBuffer, stepScriptData); err != nil {
		return nil, err
	}

	stepScript := stepScriptBuffer.String()

	// Build artifacts list
	var artifacts []*string
	artifacts = append(artifacts, bunDigest)
	artifacts = append(artifacts, builder.artifacts...)

	// Add node module artifact digests
	for _, packageName := range artifact.SortedKeys(builder.nodeModules) {
		digest := builder.nodeModules[packageName]
		if digest == nil {
			return nil, fmt.Errorf("node module %q has nil digest", packageName)
		}
		artifacts = append(artifacts, digest)
	}

	// Build environments
	environments := []string{
		fmt.Sprintf("PATH=%s", bunBin),
	}

	for _, env := range builder.environments {
		environments = append(environments, env)
	}

	// Create step and artifact
	sources := []*api.ArtifactSource{source}

	step, err := artifact.Shell(context, artifacts, environments, stepScript, builder.secrets)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifact(builder.name, steps, builder.systems).
		WithSources(sources).
		Build(context)
}
