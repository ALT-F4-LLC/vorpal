package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustStd(context *config.ConfigContext) (*string, error) {
	name := "rust-std"

	system := context.GetTarget()

	sourceTarget, err := RustToolchainTarget(system)
	if err != nil {
		return nil, err
	}

	sourceVersion := RustToolchainVersion()
	sourcePath := fmt.Sprintf("https://static.rust-lang.org/dist/%s-%s-%s.tar.gz", name, sourceVersion, *sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`cp -pr "./source/%s/%s-%s-%s/%s-%s/." "$VORPAL_OUTPUT"`, name, name, sourceVersion, *sourceTarget, name, *sourceTarget)

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
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
