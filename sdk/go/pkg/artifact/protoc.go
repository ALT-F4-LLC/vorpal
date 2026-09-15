package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Protoc(context *config.ConfigContext) (*string, error) {
	name := "protoc"

	system := context.GetTargetStr()

	var sourceTarget string
	switch system {
	case "aarch64-darwin":
		sourceTarget = "osx-aarch_64"
	case "aarch64-linux":
		sourceTarget = "linux-aarch_64"
	case "x86_64-darwin":
		sourceTarget = "osx-x86_64"
	case "x86_64-linux":
		sourceTarget = "linux-x86_64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := "34.0"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/protoc-%s-%s.zip", sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/%s/bin/protoc" "$VORPAL_OUTPUT/bin/protoc"

chmod +x "$VORPAL_OUTPUT/bin/protoc"`, name)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
