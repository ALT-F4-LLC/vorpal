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

	system := context.GetTargetStr()

	var sourceTarget string
	switch system {
	case "aarch64-darwin":
		sourceTarget = "darwin-arm64"
	case "aarch64-linux":
		sourceTarget = "linux-arm64"
	case "x86_64-darwin":
		sourceTarget = "darwin-amd64"
	case "x86_64-linux":
		sourceTarget = "linux-amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := "1.26.0"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/go%s.%s.tar.gz", sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf("cp -pr \"./source/%s/go/.\" \"$VORPAL_OUTPUT\"", name)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
