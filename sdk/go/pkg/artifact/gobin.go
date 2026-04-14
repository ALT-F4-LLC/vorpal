package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func sourceTools(name string) api.ArtifactSource {
	version := "0.42.0"
	path := fmt.Sprintf("https://sdk.vorpal.build/source/go-tools-v%s.tar.gz", version)
	return NewArtifactSource(name, path).Build()
}

func GoBin(context *config.ConfigContext) (*string, error) {
	name := "go"

	system := context.GetTarget()

	var sourceTarget string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		sourceTarget = "darwin-arm64"
	case api.ArtifactSystem_AARCH64_LINUX:
		sourceTarget = "linux-arm64"
	case api.ArtifactSystem_X8664_DARWIN:
		sourceTarget = "darwin-amd64"
	case api.ArtifactSystem_X8664_LINUX:
		sourceTarget = "linux-amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	sourceVersion := "1.26.0"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/go%s.%s.tar.gz", sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf("cp -pr \"./source/%s/go/.\" \"$VORPAL_OUTPUT\"", name)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, systems).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
