package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Git(context *config.ConfigContext) (*string, error) {
	name := "git"

	sourceVersion := "2.53.0"

	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/git-%s.tar.gz", sourceVersion)

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

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, sourceVersion)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
