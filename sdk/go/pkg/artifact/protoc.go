package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Protoc(context *config.ConfigContext) (*string, error) {
	name := "protoc"

	system := context.GetTarget()

	var sourceTarget string
	switch system {
	case api.ArtifactSystem_AARCH64_DARWIN:
		sourceTarget = "osx-aarch_64"
	case api.ArtifactSystem_AARCH64_LINUX:
		sourceTarget = "linux-aarch_64"
	case api.ArtifactSystem_X8664_DARWIN:
		sourceTarget = "osx-x86_64"
	case api.ArtifactSystem_X8664_LINUX:
		sourceTarget = "linux-x86_64"
	default:
		return nil, fmt.Errorf("unsupported %s system: %s", name, system.String())
	}

	sourceVersion := "34.0"
	sourcePath := fmt.Sprintf("https://github.com/protocolbuffers/protobuf/releases/download/v%s/protoc-%s-%s.zip", sourceVersion, sourceVersion, sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"

cp -pr "source/%s/bin/protoc" "$VORPAL_OUTPUT/bin/protoc"

chmod +x "$VORPAL_OUTPUT/bin/protoc"`, name)

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
