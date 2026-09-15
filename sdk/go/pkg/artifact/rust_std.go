package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func RustStd(context *config.ConfigContext) (*string, error) {
	name := "rust-std"

	system := context.GetTargetStr()

	sourceTarget, err := RustToolchainTarget(system)
	if err != nil {
		return nil, err
	}

	sourceVersion := RustToolchainVersion()
	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/%s-%s-%s.tar.gz", name, sourceVersion, *sourceTarget)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`cp -pr "./source/%s/%s-%s-%s/%s-%s/." "$VORPAL_OUTPUT"`, name, name, sourceVersion, *sourceTarget, name, *sourceTarget)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
