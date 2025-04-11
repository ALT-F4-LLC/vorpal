package artifact

import (
	"errors"
	"fmt"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

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

type ArtifactBuilder struct {
	Name    string
	Sources []*artifact.ArtifactSource
	Steps   []*artifact.ArtifactStep
	Systems []artifact.ArtifactSystem
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

func NewArtifactBuilder(name string) *ArtifactBuilder {
	return &ArtifactBuilder{
		Name:    name,
		Sources: []*artifact.ArtifactSource{},
		Steps:   []*artifact.ArtifactStep{},
		Systems: []artifact.ArtifactSystem{},
	}
}

func (a *ArtifactBuilder) WithSource(source *artifact.ArtifactSource) *ArtifactBuilder {
	exists := false

	for _, s := range a.Sources {
		if s == source {
			exists = true
			break
		}
	}

	if !exists {
		a.Sources = append(a.Sources, source)
	}

	return a
}

func (a *ArtifactBuilder) WithStep(step *artifact.ArtifactStep) *ArtifactBuilder {
	exists := false

	for _, s := range a.Steps {
		if s == step {
			exists = true
			break
		}
	}

	if !exists {
		a.Steps = append(a.Steps, step)
	}

	return a
}

func (a *ArtifactBuilder) WithSystem(system artifact.ArtifactSystem) *ArtifactBuilder {
	exists := false

	for _, s := range a.Systems {
		if s == system {
			exists = true
			break
		}
	}

	if !exists {
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
