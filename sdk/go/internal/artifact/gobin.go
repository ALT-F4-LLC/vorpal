package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func GoBin(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "fcbb57571c180e4db1eade2fb51d047083c44ce6acd97d7611d00d15df2d041d"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "18887d4facdc3343a40af15e07f753aaab582fbe1f2c5106dbf13a0c221b14e9"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = ""
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = ""
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
