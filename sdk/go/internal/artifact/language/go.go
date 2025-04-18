package language

import (
	"bytes"
	"fmt"
	"strings"
	"text/template"

	artifactApi "github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

type GoBuilder struct {
	artifacts      []*string
	buildDirectory *string
	buildPath      *string
	buildScripts   []string
	includes       []string
	name           string
	source         *artifactApi.ArtifactSource
}

type GoScriptTemplateArgs struct {
	BuildDirectory string
	BuildPath      string
	BuildScripts   string
	Name           string
	SourceDir      string
}

const GoScriptTemplate = `
pushd {{.SourceDir}}

mkdir -p $VORPAL_OUTPUT/bin

{{- if .BuildScripts}}
{{.BuildScripts}}
{{- end}}

pushd {{.BuildDirectory}}

go build -o $VORPAL_OUTPUT/bin/{{.Name}} {{.BuildPath}}

go clean -modcache`

func GetGOOS(target artifactApi.ArtifactSystem) string {
	var goos string

	switch target {
	case artifactApi.ArtifactSystem_AARCH64_DARWIN, artifactApi.ArtifactSystem_X8664_DARWIN:
		goos = "darwin"
	case artifactApi.ArtifactSystem_AARCH64_LINUX, artifactApi.ArtifactSystem_X8664_LINUX:
		goos = "linux"
	default:
		panic("Unsupported target system")
	}

	return goos
}

func GetGOARCH(target artifactApi.ArtifactSystem) string {
	var goarch string

	switch target {
	case artifactApi.ArtifactSystem_AARCH64_DARWIN, artifactApi.ArtifactSystem_AARCH64_LINUX:
		goarch = "arm64"
	case artifactApi.ArtifactSystem_X8664_DARWIN, artifactApi.ArtifactSystem_X8664_LINUX:
		goarch = "amd64"
	default:
		panic("Unsupported target system")
	}

	return goarch
}

func NewGoBuilder(name string) *GoBuilder {
	return &GoBuilder{
		artifacts:      []*string{},
		buildDirectory: nil,
		buildPath:      nil,
		buildScripts:   []string{},
		includes:       []string{},
		name:           name,
		source:         nil,
	}
}

func contains(slice []string, item string) bool {
	for _, s := range slice {
		if s == item {
			return true
		}
	}
	return false
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

func (b *GoBuilder) WithBuildScript(script string) *GoBuilder {
	if !contains(b.buildScripts, script) {
		b.buildScripts = append(b.buildScripts, script)
	}
	return b
}

func (b *GoBuilder) WithIncludes(includes []string) *GoBuilder {
	b.includes = includes
	return b
}

func (b *GoBuilder) WithSource(source *artifactApi.ArtifactSource) *GoBuilder {
	b.source = source
	return b
}

func (builder *GoBuilder) Build(context *config.ConfigContext) (*string, error) {
	goBin, err := artifact.GoBin(context)
	if err != nil {
		return nil, err
	}

	sourcePath := "."

	var source *artifactApi.ArtifactSource

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

	buildScripts := ""
	if len(builder.buildScripts) > 0 {
		buildScripts = strings.Join(builder.buildScripts, "\n")
	}

	stepScriptData := GoScriptTemplateArgs{
		BuildDirectory: buildDirectory,
		BuildPath:      buildPath,
		BuildScripts:   buildScripts,
		Name:           builder.name,
		SourceDir:      fmt.Sprintf("./source/%s", source.Name),
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

	environments := []string{
		"CGO_ENABLED=0",
		fmt.Sprintf("GOARCH=%s", GetGOARCH(context.GetTarget())),
		"GOCACHE=$VORPAL_WORKSPACE/go/cache",
		fmt.Sprintf("GOOS=%s", GetGOOS(context.GetTarget())),
		"GOPATH=$VORPAL_WORKSPACE/go",
		fmt.Sprintf("PATH=%s/bin", artifact.GetEnvKey(goBin)),
	}

	step, err := artifact.Shell(context, artifacts, environments, stepScript)
	if err != nil {
		return nil, err
	}

	return artifact.NewArtifactBuilder(builder.name).
		WithSource(source).
		WithStep(step).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_AARCH64_LINUX).
		WithSystem(artifactApi.ArtifactSystem_X8664_DARWIN).
		WithSystem(artifactApi.ArtifactSystem_X8664_LINUX).
		Build(context)
}
