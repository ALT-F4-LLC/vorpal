package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Git(context *config.ConfigContext) (*string, error) {
	name := "git"

	sourceVersion := "2.53.0"

	sourcePath := fmt.Sprintf("https://www.kernel.org/pub/software/scm/git/git-%s.tar.gz", sourceVersion)

	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT/bin"

pushd ./source/%s/git-%s

./configure --prefix=$VORPAL_OUTPUT

make
make install`, name, sourceVersion)

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
