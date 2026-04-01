package artifact

import (
	"fmt"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func Rsync(context *config.ConfigContext) (*string, error) {
	name := "rsync"
	version := "3.4.1"

	sourcePath := fmt.Sprintf("https://download.samba.org/pub/rsync/src/rsync-%s.tar.gz", version)
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

	systems := []api.ArtifactSystem{
		api.ArtifactSystem_AARCH64_DARWIN,
		api.ArtifactSystem_AARCH64_LINUX,
		api.ArtifactSystem_X8664_DARWIN,
		api.ArtifactSystem_X8664_LINUX,
	}

	return NewArtifact(name, []*api.ArtifactStep{step}, systems).
		WithAliases([]string{fmt.Sprintf("%s:%s", name, version)}).
		WithSources([]*api.ArtifactSource{&source}).
		Build(context)
}
