package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Pnpm(context *config.ConfigContext) (*string, error) {
	name := "pnpm"
	system := context.GetTargetStr()

	var sourceTarget string
	switch system {
	case "aarch64-darwin":
		sourceTarget = "macos-arm64"
	case "aarch64-linux":
		sourceTarget = "linux-arm64"
	case "x86_64-darwin":
		sourceTarget = "macos-x64"
	case "x86_64-linux":
		sourceTarget = "linux-x64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := "10.30.3"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/pnpm-%s-%s", sourceVersion, sourceTarget)
	source := NewArtifactSource(name, sourcePath).Build()

	sourceFilename := fmt.Sprintf("pnpm-%s-%s", sourceVersion, sourceTarget)

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/%s/%s" "$VORPAL_OUTPUT/bin/pnpm"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`, name, sourceFilename)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
