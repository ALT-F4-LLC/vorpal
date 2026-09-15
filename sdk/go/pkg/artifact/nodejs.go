package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func NodeJS(context *config.ConfigContext) (*string, error) {
	name := "nodejs"

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

	sourceVersion := "22.22.0"
	sourcePath := fmt.Sprintf(
		"https://sdk.vorpal.build/source/node-v%s-%s.tar.gz",
		sourceVersion, sourceTarget,
	)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(
		"cp -pr \"./source/%s/node-v%s-%s/.\" \"$VORPAL_OUTPUT\"",
		name, sourceVersion, sourceTarget,
	)

	step, err := Shell(context, []*string{}, []string{}, stepScript, []*api.ArtifactStepSecret{})
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
