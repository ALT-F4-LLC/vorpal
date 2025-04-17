package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Grpcurl(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "b64fdc6f8e42b27b05e6ee7c5fcf4ae1ec59584d66226279d59cc75be1b398f2"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "9066275e1c80caa531b44d9b1ccb01d57bc31945a2a2a533fb8a3db7630d6d7d"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "951378a2abade541f261b64194dfad984876d5d3f0dc51e93868b4d641491969"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "f648fc548fb7d0eccb0cb9e6f1d3209bbea5615e4a58f5f19da2b9f61be6da8b"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
