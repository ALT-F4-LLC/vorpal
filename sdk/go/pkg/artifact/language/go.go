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

type GoScriptTemplateArgs struct {
	BuildDirectory string
	BuildFlags     string
	BuildPath      string
	Name           string
	SourceDir      string
	SourceScripts  string
}

type Go struct {
	artifacts      []*string
	buildDirectory *string
	buildFlags     *string
	buildPath      *string
	environments   []string
	includes       []string
	name           string
	secrets        []*api.ArtifactStepSecret
	source         *api.ArtifactSource
	sourceScripts  []string
	systems        []api.ArtifactSystem
}

const GoScriptTemplate = `
pushd {{.SourceDir}}

mkdir -p $VORPAL_OUTPUT/bin

{{- if .SourceScripts}}
{{.SourceScripts}}
{{- end}}

go build -C {{.BuildDirectory}} -o $VORPAL_OUTPUT/bin/{{.Name}} {{.BuildFlags}} {{.BuildPath}}

go clean -modcache`

func GetGOOS(target api.ArtifactSystem) (*string, error) {
	var goos string

	switch target {
	case api.ArtifactSystem_AARCH64_DARWIN, api.ArtifactSystem_X8664_DARWIN:
		goos = "darwin"
	case api.ArtifactSystem_AARCH64_LINUX, api.ArtifactSystem_X8664_LINUX:
		goos = "linux"
	default:
		return nil, fmt.Errorf("unsupported target system: %s", target)
	}

	return &goos, nil
}

func GetGOARCH(target api.ArtifactSystem) (*string, error) {
	var goarch string

	switch target {
	case api.ArtifactSystem_AARCH64_DARWIN, api.ArtifactSystem_AARCH64_LINUX:
		goarch = "arm64"
	case api.ArtifactSystem_X8664_DARWIN, api.ArtifactSystem_X8664_LINUX:
		goarch = "amd64"
	default:
		return nil, fmt.Errorf("unsupported target system: %s", target)
	}

	return &goarch, nil
}

func NewGo(name string, systems []api.ArtifactSystem) *Go {
	return &Go{
		artifacts:      []*string{},
		buildDirectory: nil,
		buildFlags:     nil,
		buildPath:      nil,
		environments:   []string{},
		includes:       []string{},
		name:           name,
		secrets:        []*api.ArtifactStepSecret{},
		source:         nil,
		sourceScripts:  []string{},
		systems:        systems,
	}
}

func (b *Go) WithArtifacts(artifacts []*string) *Go {
	b.artifacts = artifacts
	return b
}

func (b *Go) WithBuildDirectory(directory string) *Go {
	b.buildDirectory = &directory
	return b
}

func (b *Go) WithBuildFlags(flags string) *Go {
	b.buildFlags = &flags
	return b
}

func (b *Go) WithBuildPath(path string) *Go {
	b.buildPath = &path
	return b
}

func (b *Go) WithEnvironments(environments []string) *Go {
	b.environments = environments
	return b
}

func (b *Go) WithIncludes(includes []string) *Go {
	b.includes = includes
	return b
}

func (b *Go) WithSecrets(secrets map[string]string) *Go {
	for name, value := range secrets {
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

func (b *Go) WithSource(source *api.ArtifactSource) *Go {
	b.source = source
	return b
}

func (b *Go) WithSourceScript(script string) *Go {
	if !slices.Contains(b.sourceScripts, script) {
		b.sourceScripts = append(b.sourceScripts, script)
	}
	return b
}

func (builder *Go) Build(context *config.ConfigContext) (*string, error) {
	goBin, err := artifact.GoBin(context)
	if err != nil {
		return nil, err
	}

	sourcePath := "."

	var source *api.ArtifactSource

	if builder.source != nil {
		source = builder.source
	} else {
		sourceBuilder := artifact.NewArtifactSource(builder.name, sourcePath)

		if len(builder.includes) > 0 {
			sourceBuilder = sourceBuilder.WithIncludes(builder.includes)
		}

		src := sourceBuilder.Build()

		source = &src
	}

	buildDirectory := sourcePath
	if builder.buildDirectory != nil {
		buildDirectory = *builder.buildDirectory
	}

	buildFlags := ""
	if builder.buildFlags != nil {
		buildFlags = *builder.buildFlags
	}

	buildPath := sourcePath
	if builder.buildPath != nil {
		buildPath = *builder.buildPath
	}

	sourceScripts := ""
	if len(builder.sourceScripts) > 0 {
		sourceScripts = strings.Join(builder.sourceScripts, "\n")
	}

	stepScriptData := GoScriptTemplateArgs{
		BuildDirectory: buildDirectory,
		BuildFlags:     buildFlags,
		BuildPath:      buildPath,
		Name:           builder.name,
		SourceDir:      fmt.Sprintf("./source/%s", source.Name),
		SourceScripts:  sourceScripts,
	}

	tmpl, err := template.New("script").Parse(GoScriptTemplate)
	if err != nil {
		return nil, err
	}

	var stepScriptBuffer bytes.Buffer

	if err := tmpl.Execute(&stepScriptBuffer, stepScriptData); err != nil {
		return nil, err
	}

	stepScript := stepScriptBuffer.String()

	var artifacts []*string
	artifacts = append(artifacts, goBin)
	artifacts = append(artifacts, builder.artifacts...)

	system := context.GetTarget()

	goarch, err := GetGOARCH(system)
	if err != nil {
		return nil, err
	}

	goos, err := GetGOOS(system)
	if err != nil {
		return nil, err
	}

	environments := []string{
		fmt.Sprintf("GOARCH=%s", *goarch),
		"GOCACHE=$VORPAL_WORKSPACE/go/cache",
		fmt.Sprintf("GOOS=%s", *goos),
		"GOPATH=$VORPAL_WORKSPACE/go",
		fmt.Sprintf("PATH=%s/bin", artifact.GetEnvKey(goBin)),
	}

	for _, env := range builder.environments {
		environments = append(environments, env)
	}

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
