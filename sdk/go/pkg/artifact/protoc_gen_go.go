package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	name := "protoc-gen-go"
	system := context.GetTargetStr()

	var sourceTarget string

	switch system {
	case "aarch64-darwin":
		sourceTarget = "darwin.arm64"
	case "aarch64-linux":
		sourceTarget = "linux.arm64"
	case "x86_64-darwin":
		sourceTarget = "darwin.amd64"
	case "x86_64-linux":
		sourceTarget = "linux.amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system)
	}

	sourceVersion := "1.36.11"
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/protoc-gen-go.v%s.%s.tar.gz", sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := `mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/protoc-gen-go/protoc-gen-go" "$VORPAL_OUTPUT/bin/protoc-gen-go"

chmod +x "$VORPAL_OUTPUT/bin/protoc-gen-go"`

	step, err := Shell(context, []*string{}, []string{}, stepScript, []*api.ArtifactStepSecret{})
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}

	return NewArtifact(name, steps, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
