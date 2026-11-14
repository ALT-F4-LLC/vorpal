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
		sourceTarget = "darwin-arm64"
	case "aarch64-linux":
		sourceTarget = "linux-arm64"
	case "x86_64-darwin":
		sourceTarget = "darwin-x64"
	case "x86_64-linux":
		sourceTarget = "linux-x64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := "12.21.0"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/pnpm-%s-%s.tar.gz", sourceVersion, sourceTarget)
	source := NewArtifactSource(name, sourcePath).Build()

	// The archive holds the pnpm executable and the dist tree it loads from
	// its own directory, so both land in bin, as pnpm's installer lays out
	// PNPM_HOME.
	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"
cp -pR "./source/%s/." "$VORPAL_OUTPUT/bin/"
chmod +x "$VORPAL_OUTPUT/bin/pnpm"`, name)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
