package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gh(context *config.ConfigContext) (*string, error) {
	name := "gh"
	system := context.GetTarget()

	var sourceTarget string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		sourceTarget = "macOS_arm64"
	case api.ArtifactSystem_AARCH64_LINUX:
		sourceTarget = "linux_arm64"
	case api.ArtifactSystem_X8664_DARWIN:
		sourceTarget = "macOS_amd64"
	case api.ArtifactSystem_X8664_LINUX:
		sourceTarget = "linux_amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	var sourceExtension string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN, api.ArtifactSystem_X8664_DARWIN:
		sourceExtension = "zip"
	case api.ArtifactSystem_AARCH64_LINUX, api.ArtifactSystem_X8664_LINUX:
		sourceExtension = "tar.gz"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	sourceVersion := "2.87.3"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/gh_%s_%s.%s", sourceVersion, sourceTarget, sourceExtension)
	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/%s/gh_%s_%s/bin/gh" "$VORPAL_OUTPUT/bin/gh"

chmod +x "$VORPAL_OUTPUT/bin/gh"`, name, sourceVersion, sourceTarget)

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
