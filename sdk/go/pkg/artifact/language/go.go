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
	BuildPath      string
	Name           string
	SourceDir      string
	SourceScripts  string
}

type GoBuilder struct {
	artifacts      []*string
	buildDirectory *string
	buildPath      *string
	includes       []string
	name           string
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

go build -C {{.BuildDirectory}} -o $VORPAL_OUTPUT/bin/{{.Name}} {{.BuildPath}}

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

func NewGoBuilder(name string, systems []api.ArtifactSystem) *GoBuilder {
	return &GoBuilder{
		artifacts:      []*string{},
		buildDirectory: nil,
		buildPath:      nil,
		sourceScripts:  []string{},
		includes:       []string{},
		name:           name,
		source:         nil,
		systems:        systems,
	}
}

func (b *GoBuilder) WithArtifacts(artifacts []*string) *GoBuilder {
	b.artifacts = artifacts
	return b
}

func (b *GoBuilder) WithBuildDirectory(directory string) *GoBuilder {
	b.buildDirectory = &directory
	return b
}

func (b *GoBuilder) WithBuildPath(path string) *GoBuilder {
	b.buildPath = &path
	return b
}

func (b *GoBuilder) WithIncludes(includes []string) *GoBuilder {
	b.includes = includes
	return b
}

func (b *GoBuilder) WithSource(source *api.ArtifactSource) *GoBuilder {
	b.source = source
	return b
}

func (b *GoBuilder) WithSourceScript(script string) *GoBuilder {
	if !slices.Contains(b.sourceScripts, script) {
		b.sourceScripts = append(b.sourceScripts, script)
	}
	return b
}

func (builder *GoBuilder) Build(context *config.ConfigContext) (*string, error) {
	goBin, err := artifact.GoBin(context)
	if err != nil {
		return nil, err
	}

	sourcePath := "."

	var source *api.ArtifactSource

	if builder.source != nil {
		source = builder.source
	} else {
		sourceBuilder := artifact.NewArtifactSourceBuilder(builder.name, sourcePath)

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

	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	goarch, err := GetGOARCH(*target)
	if err != nil {
		return nil, err
	}

	goos, err := GetGOOS(*target)
	if err != nil {
		return nil, err
	}

	environments := []string{
		"CGO_ENABLED=0",
		fmt.Sprintf("GOARCH=%s", *goarch),
		"GOCACHE=$VORPAL_WORKSPACE/go/cache",
		fmt.Sprintf("GOOS=%s", *goos),
		"GOPATH=$VORPAL_WORKSPACE/go",
		fmt.Sprintf("PATH=%s/bin", artifact.GetEnvKey(goBin)),
	}

	step, err := artifact.Shell(context, artifacts, environments, stepScript)
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return artifact.NewArtifactBuilder(builder.name, steps, builder.systems).
		WithSource(source).
		Build(context)
}
