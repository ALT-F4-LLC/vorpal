package artifact

import (
	"errors"

	api "github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config"
)

func GoBin(context *config.ConfigContext) (*string, error) {
	target, err := context.GetTarget()
	if err != nil {
		return nil, err
	}

	var digest string

	switch *target {
	case api.ArtifactSystem_AARCH64_DARWIN:
		digest = "fcbb57571c180e4db1eade2fb51d047083c44ce6acd97d7611d00d15df2d041d"
	case api.ArtifactSystem_AARCH64_LINUX:
		digest = "bb2c34e221c765c8ffdd740e0b0ee1f13fdac59f0836a06ef9d335c8787d6f0d"
	case api.ArtifactSystem_X8664_DARWIN:
		digest = "24614ca2fd7a86cb515cffa2c519965326ad73fb520c09581eab70623537a041"
	case api.ArtifactSystem_X8664_LINUX:
		digest = "219b84a4fe05827674fc1ca51d738026d1f95f27c2487b6218dfc8e8d7779406"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
