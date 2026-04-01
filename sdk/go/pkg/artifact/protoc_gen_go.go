package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func ProtocGenGo(context *config.ConfigContext) (*string, error) {
	name := "protoc-gen-go"
	system := context.GetTarget()

	var sourceTarget string

	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		sourceTarget = "darwin.arm64"
	case api.ArtifactSystem_AARCH64_LINUX:
		sourceTarget = "linux.arm64"
	case api.ArtifactSystem_X8664_DARWIN:
		sourceTarget = "darwin.amd64"
	case api.ArtifactSystem_X8664_LINUX:
		sourceTarget = "linux.amd64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	sourceVersion := "1.36.11"
	sourcePath := fmt.Sprintf("https://github.com/protocolbuffers/protobuf-go/releases/download/v%s/protoc-gen-go.v%s.%s.tar.gz", sourceVersion, sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := `mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/protoc-gen-go/protoc-gen-go" "$VORPAL_OUTPUT/bin/protoc-gen-go"

chmod +x "$VORPAL_OUTPUT/bin/protoc-gen-go"`

	step, err := Shell(context, []*string{}, []string{}, stepScript, []*api.ArtifactStepSecret{})
	if err != nil {
		return nil, err
	}

	steps := []*api.ArtifactStep{step}
	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(name, steps, systems).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
