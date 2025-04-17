package artifact

import (
	"errors"

	"github.com/ALT-F4-LLC/vorpal/sdk/go/api/v0/artifact"
	"github.com/ALT-F4-LLC/vorpal/sdk/go/internal/config"
)

func Staticcheck(context *config.ConfigContext) (*string, error) {
	target := context.GetTarget()

	var digest string

	switch target {
	case artifact.ArtifactSystem_AARCH64_DARWIN:
		digest = "cf91f83328ee0338627e8a4f00bc487ee77f8db793fc6655d2660c88641884c7"
	case artifact.ArtifactSystem_AARCH64_LINUX:
		digest = "faa7c723e66d35c085dc4ebb2dfe18eae2b5538a176c55867576d35932e2a138"
	case artifact.ArtifactSystem_X8664_DARWIN:
		digest = "5a6c598856065cbbc76ae0d276962589bd09f3bec97eb8448370ee3bbe3f7853"
	case artifact.ArtifactSystem_X8664_LINUX:
		digest = "b3740debcd9089b0cbb9a90ecf45e63e8c1182cf877ec448914476d0e2c49d8f"
	default:
		return nil, errors.New("unsupported target")
	}

	return context.FetchArtifact(digest)
}
