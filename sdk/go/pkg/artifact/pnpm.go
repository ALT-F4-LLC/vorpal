package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Pnpm(context *config.ConfigContext) (*string, error) {
	name := "pnpm"
	system := context.GetTarget()

	var sourceTarget string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		sourceTarget = "macos-arm64"
	case api.ArtifactSystem_AARCH64_LINUX:
		sourceTarget = "linux-arm64"
	case api.ArtifactSystem_X8664_DARWIN:
		sourceTarget = "macos-x64"
	case api.ArtifactSystem_X8664_LINUX:
		sourceTarget = "linux-x64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	sourceVersion := "10.30.3"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/pnpm-%s-%s", sourceVersion, sourceTarget)
	source := NewArtifactSource(name, sourcePath).Build()

	// macos-arm64 ships with the version in the filename; all other targets do not.
	sourceFilename := fmt.Sprintf("pnpm-%s", sourceTarget)
	if system == api.ArtifactSystem_AARCH64_DARWIN {
		sourceFilename = fmt.Sprintf("pnpm-%s-%s", sourceVersion, sourceTarget)
	}

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/%s/%s" "$VORPAL_OUTPUT/bin/pnpm"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`, name, sourceFilename)

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
