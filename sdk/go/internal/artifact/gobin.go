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
		digest = "24614ca2fd7a86cb515cffa2c519965326ad73fb520c09581eab70623537a041"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "219b84a4fe05827674fc1ca51d738026d1f95f27c2487b6218dfc8e8d7779406"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
