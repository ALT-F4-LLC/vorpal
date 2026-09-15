package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Gh(context *config.ConfigContext) (*string, error) {
	name := "gh"
	system := context.GetTargetStr()

	var sourceTarget string
	switch system {
	case "aarch64-darwin":
		sourceTarget = "macOS_arm64"
	case "aarch64-linux":
		sourceTarget = "linux_arm64"
	case "x86_64-darwin":
		sourceTarget = "macOS_amd64"
	case "x86_64-linux":
		sourceTarget = "linux_amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	var sourceExtension string
	switch system {
	case "aarch64-darwin", "x86_64-darwin":
		sourceExtension = "zip"
	case "aarch64-linux", "x86_64-linux":
		sourceExtension = "tar.gz"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
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

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
