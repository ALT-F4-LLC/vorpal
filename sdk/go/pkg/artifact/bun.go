package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

const defaultBunVersion = "1.3.10"

func Bun(context *config.ConfigContext) (*string, error) {
	name := "bun"

	system := context.GetTargetStr()

	var sourceTarget string
	switch system {
	case "aarch64-darwin":
		sourceTarget = "darwin-aarch64"
	case "aarch64-linux":
		sourceTarget = "linux-aarch64"
	case "x86_64-darwin":
		sourceTarget = "darwin-x64"
	case "x86_64-linux":
		sourceTarget = "linux-x64-baseline"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := defaultBunVersion
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/bun-%s-%s.zip", sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"
cp -p "./source/%s/bun-%s/bun" "$VORPAL_OUTPUT/bin/bun"
chmod +x "$VORPAL_OUTPUT/bin/bun"
`, name, sourceTarget)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
