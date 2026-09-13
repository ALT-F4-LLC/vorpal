package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Rsync(context *config.ConfigContext) (*string, error) {
	name := "rsync"
	version := "3.4.1"

	sourcePath := fmt.Sprintf("https://sdk.vorpal.build/source/rsync-%s.tar.gz", version)
	source := NewArtifactSource(name, sourcePath).Build()

	stepScript := fmt.Sprintf(`mkdir -p "$VORPAL_OUTPUT"
pushd ./source/%s/%s-%s
./configure --prefix="$VORPAL_OUTPUT" --disable-openssl --disable-xxhash --disable-zstd --disable-lz4
make
make install`, name, name, version)

	step, err := Shell(context, []*string{}, []string{}, stepScript, nil)
	if err != nil {
		return nil, err
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, config.SYSTEMS).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, version)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
