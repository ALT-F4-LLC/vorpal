package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Goimports(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "112f8c42be33bfa5274fcdff2748cd68eae755adbae1cbd70cc012531375d7c1"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "6562e6c4c50f955c7d808860b383261446525f1bcfccd1b9ac5f8f58af7d8842"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "66f03633619da58f2378753782ccd00d652a084363758ef1c81eb9212bafa16a"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "e780f99be7bb2efc9aee6a542cf81994a94f17ceaeda9cbd28e41ac5ccb48cb0"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
